use std::str::FromStr;

#[derive(Debug)]
pub struct PartitionTable {
    strategy: u8,
    unit: u8,
    partitions: Vec<Partition>,
}

impl PartitionTable {
    pub fn new(strategy: u8, unit: u8) -> Self {
        Self {
            strategy,
            unit,
            partitions: Vec::new(),
        }
    }

    pub fn strategy(&self) -> u8 {
        self.strategy
    }

    pub fn unit(&self) -> u8 {
        self.unit
    }

    pub fn add_partition(&mut self, partition: Partition) {
        self.partitions.push(partition);
    }

    pub fn partitions(&self) -> &[Partition] {
        &self.partitions
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Add header
        bytes.extend_from_slice(&[0x70, 0x61, 0x72, 0x3a, self.strategy, self.unit]); //"par:"" strategy, unit
        bytes.extend_from_slice(&(self.partitions.len() as u16).to_le_bytes());
        for partition in &self.partitions {
            bytes.extend_from_slice(&partition.to_bytes());
        }
        bytes
    }
}

#[derive(Debug)]
pub struct Partition {
    name: String,
    gap: u64,
    size: u64,
}

impl Partition {
    pub fn new(name: String, gap: u64, size: u64) -> Self {
        Self { name, gap, size }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn gap(&self) -> u64 {
        self.gap
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn to_bytes(&self) -> [u8; 0x58] {
        let mut bytes = [0u8; 0x58];
        let name_utf16: Vec<u8> = str::encode_utf16(&self.name)
            .map(|c| [(c & 0xff) as u8, (c >> 8) as u8])
            .flatten()
            .collect();
        if name_utf16.len() > 0x40 {
            panic!("Partition name is too long");
        }
        bytes[..name_utf16.len()].copy_from_slice(&name_utf16);
        bytes[0x40..0x48].copy_from_slice(&self.gap.to_le_bytes());
        bytes[0x48..0x50].copy_from_slice(&self.size.to_le_bytes());
        bytes
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ImageType {
    Init,
    Eip,
    Fdl1,
    Fdl2,
    EraseFlash,
    Code,
}

impl FromStr for ImageType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "INIT" => Ok(Self::Init),
            "EIP" => Ok(Self::Eip),
            "FDL1" => Ok(Self::Fdl1),
            "FDL2" => Ok(Self::Fdl2),
            "ERASEFLASH" => Ok(Self::EraseFlash),
            "CODE" => Ok(Self::Code),
            _ => Err(()),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Block {
    Absolute(u64),
    Partition(String),
}

#[derive(Debug)]
pub struct Image {
    flag: u32,
    name: String,
    r#type: ImageType,
    block: Block,
    file: Option<String>,
    description: String,
}
impl Image {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn r#type(&self) -> ImageType {
        self.r#type
    }

    pub(crate) fn block(&self) -> &Block {
        &self.block
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }
}

#[derive(Debug)]
pub struct Project {
    partition_table: PartitionTable,
    images: Vec<Image>,
}

impl Project {
    pub fn partition_table(&self) -> &PartitionTable {
        &self.partition_table
    }

    pub fn images(&self) -> &[Image] {
        &self.images
    }
}

pub mod deserialize {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(rename = "Config")]
    pub struct Config {
        #[serde(rename = "Project")]
        pub project: Project,
    }

    #[derive(Debug, Deserialize)]
    pub struct Project {
        #[serde(rename = "alias")]
        alias: String,
        #[serde(rename = "name")]
        name: String,
        #[serde(rename = "version")]
        version: String,

        #[serde(rename = "FDLLevel")]
        fdl_level: u32,

        #[serde(rename = "Partitions")]
        partitions: Partitions,

        #[serde(rename = "ImgList")]
        img_list: ImgList,
    }

    impl From<Project> for super::Project {
        fn from(project: Project) -> super::Project {
            let partition_table = project.partitions.into();
            let mut images = Vec::new();
            for img in project.img_list.images {
                images.push(img.into());
            }
            super::Project {
                partition_table,
                images,
            }
        }
    }

    #[derive(Debug, Deserialize)]
    struct Partitions {
        #[serde(rename = "strategy")]
        strategy: u32,
        #[serde(rename = "unit")]
        unit: u32,

        #[serde(rename = "$value")]
        partitions: Vec<Partition>,
    }

    impl From<Partitions> for super::PartitionTable {
        fn from(partitions: Partitions) -> super::PartitionTable {
            let mut partition_table =
                super::PartitionTable::new(partitions.strategy as u8, partitions.unit as u8);
            for partition in partitions.partitions {
                partition_table.add_partition(partition.into());
            }
            partition_table
        }
    }

    #[derive(Debug, Deserialize)]
    struct Partition {
        #[serde(rename = "gap")]
        gap: u64,
        #[serde(rename = "id")]
        id: String,
        #[serde(rename = "size")]
        size: u64,
    }

    impl From<Partition> for super::Partition {
        fn from(partition: Partition) -> Self {
            super::Partition::new(partition.id, partition.gap, partition.size)
        }
    }

    #[derive(Debug, Deserialize)]
    struct ImgList {
        #[serde(rename = "Img")]
        images: Vec<Img>,
    }

    #[derive(Debug, Deserialize)]
    struct Img {
        #[serde(rename = "flag")]
        flag: u32,
        #[serde(rename = "name")]
        name: String,
        #[serde(rename = "select")]
        select: u32,

        #[serde(rename = "ID")]
        id: String,
        #[serde(rename = "Type")]
        img_type: String,

        #[serde(rename = "Block")]
        block: Block,

        #[serde(rename = "File", deserialize_with = "empty_string_to_none")]
        file: Option<String>,

        #[serde(rename = "Auth")]
        auth: Auth,

        #[serde(rename = "Description")]
        description: String,
    }

    impl From<Img> for super::Image {
        fn from(img: Img) -> super::Image {
            super::Image {
                flag: img.flag,
                name: img.name,
                r#type: img.img_type.parse().unwrap(),
                block: img.block.into(),
                file: img.file,
                description: img.description,
            }
        }
    }

    fn from_hex<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
    }

    fn empty_string_to_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: Option<String> = Option::deserialize(deserializer)?;
        match s {
            Some(ref value) if value.is_empty() => Ok(None),
            other => Ok(other),
        }
    }

    #[derive(Debug, Deserialize)]
    struct Block {
        #[serde(rename = "id")]
        id: Option<String>,

        #[serde(rename = "Base", deserialize_with = "from_hex")]
        base: u64,

        #[serde(rename = "Size", deserialize_with = "from_hex")]
        size: u64,
    }

    impl From<Block> for super::Block {
        fn from(block: Block) -> super::Block {
            if let Some(id) = block.id {
                super::Block::Partition(id)
            } else {
                super::Block::Absolute(block.base)
            }
        }
    }

    #[derive(Debug, Deserialize)]
    struct Auth {
        #[serde(rename = "algo")]
        algo: u32,
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn test_deserialize() {
            let xml_data = r#"
        <Config>
        <Project alias="AX620E" name="AX630C" version="V2.0.0_P7_20240513101106_20250206093423">
            <FDLLevel>2</FDLLevel>
            <Partitions strategy="1" unit="2">
            <Partition gap="0" id="spl" size="768" />
            <Partition gap="0" id="ddrinit" size="512" />
            </Partitions>
            <ImgList>
            <Img flag="2" name="INIT" select="1">
                <ID>INIT</ID>
                <Type>INIT</Type>
                <Block>
                <Base>0x0</Base>
                <Size>0x0</Size>
                </Block>
                <File />
                <Auth algo="0" />
                <Description>Handshake with romcode</Description>
            </Img>
            </ImgList>
        </Project>
        </Config>
        "#;

            let config: Config = serde_xml_rs::from_str(xml_data).unwrap();
            println!("{:#?}", config);

            let project = super::super::Project::from(config.project);
            println!("{:#?}", project);
            assert_eq!(project.partition_table().strategy(), 1);
            assert_eq!(project.partition_table().unit(), 2);
            assert_eq!(project.partition_table().partitions().len(), 2);
            assert_eq!(project.partition_table().partitions()[0].name(), "spl");
            assert_eq!(project.partition_table().partitions()[0].gap(), 0);
            assert_eq!(project.partition_table().partitions()[0].size(), 768);
            assert_eq!(project.partition_table().partitions()[1].name(), "ddrinit");
            assert_eq!(project.partition_table().partitions()[1].gap(), 0);
            assert_eq!(project.partition_table().partitions()[1].size(), 512);
            assert_eq!(project.images().len(), 1);
            assert_eq!(project.images()[0].flag, 2);
            assert_eq!(project.images()[0].name, "INIT");
            assert_eq!(project.images()[0].r#type, super::super::ImageType::Init);
            assert_eq!(project.images()[0].block, super::super::Block::Absolute(0));
            assert_eq!(project.images()[0].file, None);
            assert_eq!(project.images()[0].description, "Handshake with romcode");
        }
    }
}

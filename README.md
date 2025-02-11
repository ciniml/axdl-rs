# axdl-rs Unofficial Axera image downloader implementation in Rust

This is an unofficial Axera image downloader implementation in Rust to write image file into Axera SoCs.

## Table of Contents

- [Build](#build)
- [Usage](#usage)
- [License](#license)

## Build

Before building the project, install the Rust toolchain via rustup.

```bash
# Clone the repository
git clone https://github.com/ciniml/axdl-rs.git

# Change directory
cd axdl-rs

# Build
cargo build
```

## Usage

To burn a *.axp image, run the command below and plug the Axera SoC device with download mode.
For M5Stack Module LLM, kepp press the BOOT button and plug the USB cable into the device.

```shell
cargo run -- --file /path/to/image.axp --wait-device
```

If you don't want to burn the rootfs, specify `--exclude-rootfs` option.

```shell
cargo run -- --file /path/to/image.axp --wait-device --exclude-rootfs
```

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.
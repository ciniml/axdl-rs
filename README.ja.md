# axdl-rs 非公式のAxeraイメージダウンローダーのRust実装

これは、Axera SoCにイメージファイルを書き込むための非公式のAxeraイメージダウンローダーのRust実装です。

[English](./README.md)

## 目次

- [準備](#準備)
- [インストール](#インストール)
- [ビルド](#ビルド)
- [使用方法](#使用方法)
- [ライセンス](#ライセンス)

## 準備

### Linux

通常のユーザーがデバイスにアクセスできるようにするためには、udevを設定して通常のユーザーがデバイスにアクセスできるようにする必要があります。
udevを設定するには、`99-axdl.rules`を`/etc/udev/rules.d`にコピーし、udevの設定をリロードします。

```
sudo cp 99-axdl.rules /etc/udev/rules.d/
sudo udevadm control --reload
```

## インストール

`axdl-cli` は `cargo install` にてインストールできます。

```
cargo install axdl-cli
```

## ビルド

プロジェクトをビルドする前に、rustupを使用してRustツールチェーンをインストールします。

```bash
# リポジトリをクローン
git clone https://github.com/ciniml/axdl-rs.git

# ディレクトリを変更
cd axdl-rs

# ビルド
cargo build
```

## 使用方法

*.axpイメージを書き込むには、以下のコマンドを実行し、ダウンロードモードでAxera SoCデバイスを接続します。
M5Stack Module LLMの場合、BOOTボタンを押し続けながらUSBケーブルをデバイスに接続します。

```shell
cargo run --bin axdl-cli -- --file /path/to/image.axp --wait-for-device
```

rootfsを書き込みたくない場合は、`--exclude-rootfs`オプションを指定します。

```shell
cargo run --bin axdl-cli -- --file /path/to/image.axp --wait-for-device --exclude-rootfs
```

Windows上など、AxeraのAXDL用公式ドライバをインストールしている環境で使用するには、 `--transport serial` を指定してシリアルポート経由でアクセスするようにします。

```shell
cargo run --bin axdl-cli -- --file /path/to/image.axp --wait-for-device --transport serial
```

## ライセンス

このプロジェクトはApache License 2.0の下でライセンスされています。詳細については[LICENSE](LICENSE)ファイルを参照してください。
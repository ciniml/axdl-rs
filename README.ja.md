# axdl-rs 非公式のAxeraイメージダウンローダーのRust実装

これは、Axera SoCにイメージファイルを書き込むための非公式のAxeraイメージダウンローダーのRust実装です。

[English](./README.md)

## 目次

- [準備](#準備)
- [インストール](#インストール)
- [Webブラウザ版](#Webブラウザ版)
- [ビルド](#ビルド)
- [使用方法](#使用方法)
- [ライセンス](#ライセンス)

## 準備

### Linux (Debian系)

通常のユーザーがデバイスにアクセスできるようにするためには、udevを設定して通常のユーザーがデバイスにアクセスできるようにする必要があります。
udevを設定するには、`99-axdl.rules`を`/etc/udev/rules.d`にコピーし、udevの設定をリロードします。

```
sudo cp 99-axdl.rules /etc/udev/rules.d/
sudo udevadm control --reload
```

ユーザーが `plugdev` に属していないなら、 `plugdev` に追加しててログインしなおします。 (ログインしなおさないとグループの変更が有効にならない)

```
id 
# 結果に ...,(plugdev),... が含まれているか確認する
```

```
# plugdevグループにユーザーを追加
sudo usermod -a -G plugdev $USER
```

libusbとlibudevに依存しているのでインストールしておきます。

```
sudo apt install -y libudev-dev libusb-1.0-0-dev 
```

## インストール

`axdl-cli` は `cargo install` にてインストールできます。

```
cargo install axdl-cli
```

## Webブラウザ版

Webブラウザ版は [https://www.fugafuga.org/axdl-rs/axdl-gui/latest/](https://www.fugafuga.org/axdl-rs/axdl-gui/latest/) から実行できます。

![axdl-gui](./doc/axdl-gui.drawio.svg)

1. `Open Image` を押して書き込みたい `.axp` ファイルを選択します。
2. rootfsを書き込みたくないなら `Exclude rootfs` にチェックを入れます。
3. `Open Device` を押してUSBデバイス選択画面を表示します
4. Axera SoCをダウンロードモードでホストに接続します。(M5Stack Module LLMの場合は、BOOTボタンを押しながらUSBケーブルを挿しこみます)
5. Axera SoCがダウンロードモードで動作している間に `Download` ボタンを押します。 (10秒くらいでダウンロードモードから抜けてしまうので、その場合は (3) からやり直します。)

## ビルド

### 準備

プロジェクトをビルドする前に、rustupを使用してRustツールチェーンをインストールします。

```bash
# リポジトリをクローン
git clone https://github.com/ciniml/axdl-rs.git

# ディレクトリを変更
cd axdl-rs
```

### コマンドライン版のビルド

```
# ビルド
cargo build --bin axdl-cli --package axdl-cli
```

### Webブラウザ版のビルド

Webブラウザ版のビルドには `wasm-pack` が必要なのでインストールします。

```
cargo install wasm-pack
```

`wasm-pack` を使ってビルドします。

```
cd axdl-gui
wasm-pack build --target web --release
```

## 使用方法

### コマンドライン版

*.axpイメージを書き込むには、以下のコマンドを実行し、ダウンロードモードでAxera SoCデバイスを接続します。
M5Stack Module LLMの場合、BOOTボタンを押し続けながらUSBケーブルをデバイスに接続します。

```shell
cargo run --bin axdl-cli --package axdl-cli --release -- --file /path/to/image.axp --wait-for-device
```

rootfsを書き込みたくない場合は、`--exclude-rootfs`オプションを指定します。

```shell
cargo run --bin axdl-cli --package axdl-cli --release -- --file /path/to/image.axp --wait-for-device --exclude-rootfs
```

Windows上など、AxeraのAXDL用公式ドライバをインストールしている環境で使用するには、 `--transport serial` を指定してシリアルポート経由でアクセスするようにします。

```shell
cargo run --bin axdl-cli --package axdl-cli --release -- --file /path/to/image.axp --wait-for-device --transport serial
```

### Webブラウザ版

Webブラウザ版を実行するにはビルド後、ローカルでHTTPサーバーを立ち上げるなどをしてブラウザからアクセスします。
Chrome等、WebUSBに対応したブラウザが必要です。
pythonのHTTPモジュールを使ってHTTPサーバーを立ち上げてる例を示します。

```
# Webブラウザ版のビルド
cd axdl-gui
wasm-pack build --target web --release
# HTTPサーバーを立ち上げる
python -m http.server 8000
```

[http://localhost:8000](http://localhost:8000) にアクセスするとWebブラウザ版が開きます。

## ライセンス

このプロジェクトはApache License 2.0の下でライセンスされています。詳細については[LICENSE](LICENSE)ファイルを参照してください。
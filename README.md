# axdl-rs Unofficial Axera image downloader implementation in Rust

This is an unofficial Axera image downloader implementation in Rust to write image file into Axera SoCs.

[日本語](./README.ja.md)

## Table of Contents

- [Prepare](#Prepare)
- [Install](#Install)
- [Build](#build)
- [Usage](#usage)
- [License](#license)

## Prepare

### Linux (Debian based)

In order to access to the device from a normal user, you have to configure udev to allow a normal user to access the device.
To configure udev, copy `99-axdl.rules` into `/etc/udev/rules.d` and reload the configuration of udev.

```
sudo cp 99-axdl.rules /etc/udev/rules.d/
sudo udevadm control --reload
```

If the user is not in the plugdev group, add them to it and re-login. (Group membership changes require a re-login to take effect.)

```
id
# Confirm that ...,(plugdev),... is included in the output
```

```
# Add the user to the plugdev group
sudo usermod -a -G plugdev $USER
```

Since this tool depends on libusb and libudev, install them in advance:

```
sudo apt install -y libudev-dev libusb-1.0-0-dev
```

## Install

You can run the web browser version from [https://www.fugafuga.org/axdl-rs/axdl-gui/latest/](https://www.fugafuga.org/axdl-rs/axdl-gui/latest/).

`axdl-cli` can be installed via `cargo install`.

```
cargo install axdl-cli
```

## Build

Before building the project, install the Rust toolchain via rustup.

```bash
# Clone the repository
git clone https://github.com/ciniml/axdl-rs.git

# Change directory
cd axdl-rs
```
### Building the Command-Line Version

```
# Build
cargo build --bin axdl-cli --package axdl-cli
```

### Building the Web Browser Version

To build the web browser version, install wasm-pack:

```
cargo install wasm-pack
```

Then build with wasm-pack:

```
cd axdl-gui
wasm-pack build --target web --release
```

## Usage

To burn a *.axp image, run the command below and plug the Axera SoC device with download mode.
For M5Stack Module LLM, keep press the BOOT button and plug the USB cable into the device.

```shell
cargo run --bin axdl-cli --package axdl-cli -- --file /path/to/image.axp --wait-for-device
```

If you don't want to burn the rootfs, specify `--exclude-rootfs` option.

```shell
cargo run --bin axdl-cli --package axdl-cli -- --file /path/to/image.axp --wait-for-device --exclude-rootfs
```

On Windows or other platforms where the official Axera AXDL driver is installed, you can use serial port access by specifying the --transport serial option:

```shell
cargo run --bin axdl-cli --package axdl-cli -- --file /path/to/image.axp --wait-for-device --transport serial
```

### Web Browser Version

After building, start a local HTTP server and access it from your browser. 
A browser supporting WebUSB (such as Chrome) is required. 
Below is an example of using Python’s HTTP module:

```
# Build the web browser version
cd axdl-gui
wasm-pack build --target web --release
# Start the HTTP server
python -m http.server 8000
```

Access http://localhost:8000 to open the web browser version.

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.
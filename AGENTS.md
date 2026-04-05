# AGENTS.md - AI Coding Agent Guide

This document provides essential information for AI coding agents working on this project.

## Project Overview

This is an embedded Rust firmware project for the **LilyGo T-Embed-CC1101** device, a handheld gadget based on the ESP32-S3 microcontroller. The project implements a terminal UI (TUI) application using `ratatui` rendered on the device's 1.9-inch ST7789 display.

### Target Hardware

| Component | Specification |
|-----------|---------------|
| MCU | ESP32-S3-WROOM-1 |
| Flash | 16MB |
| PSRAM | 8MB (OPI mode) |
| Display | ST7789, 170×320 resolution |
| Input | EC11 rotary encoder with push button + 2 additional buttons |
| Wireless | CC1101 (Sub-GHz), PN532 (NFC/RFID 13.56MHz) |
| Battery | 3.7V 1300mAh with BQ25896/BQ27220 power management |
| Other | WS2812 LEDs, IR transceiver, SD card slot, speaker, microphone |

### Pin Mappings

```
Display:
  BL   -> GPIO21  (Backlight)
  CS   -> GPIO41
  MOSI -> GPIO9
  SCK  -> GPIO11
  DC   -> GPIO16

Encoder:
  INA  -> GPIO4   (Rotation A)
  INB  -> GPIO5   (Rotation B)
  KEY  -> GPIO0   (Encoder push button)

Buttons:
  Confirm -> GPIO0 (shared with encoder key)
  Back    -> GPIO6

I2C:
  SDA  -> GPIO8
  SCL  -> GPIO18

SPI (Shared):
  SCK  -> GPIO11
  MOSI -> GPIO9
  MISO -> GPIO10
```

## Technology Stack

- **Language**: Rust (Edition 2024, MSRV 1.88)
- **Target**: `xtensa-esp32s3-none-elf`
- **HAL**: `esp-hal` v1.0 with ESP32-S3 support
- **Async Runtime**: Embassy via `esp-rtos`
- **UI Framework**: `ratatui` v0.30 with `mousefood` embedded backend
- **Display Driver**: `mipidsi` for ST7789
- **Debug Output**: RTT (Real-Time Transfer) via `rtt-target`

## Project Structure

```
.
├── Cargo.toml          # Project dependencies and profile settings
├── Cargo.lock          # Locked dependency versions
├── build.rs            # Build script for linker configuration
├── rust-toolchain.toml # Rust toolchain: "esp"
├── .cargo/
│   └── config.toml     # Cargo config: target, runner, rustflags
├── .clippy.toml        # Clippy config: stack-size-threshold = 1024
├── src/
│   ├── main.rs         # Entry point, hardware init, main loop
│   ├── input.rs        # Rotary encoder and button input handling
│   ├── ui.rs           # UI components (menu, brightness popup)
│   └── backlight.rs    # Brightness control channel (stub)
└── ref_doc/
    └── T-Embed-CC1101  # Symlink to reference docs (hardware specs, etc.)
```

### Module Responsibilities

- **`main.rs`**: Initializes hardware (SPI, GPIO, display), sets up the Embassy executor, creates the terminal backend, and runs the main UI loop.
- **`input.rs`**: Handles EC11 rotary encoder state machine and button debouncing. Uses async edge detection and sends events through `ENCODER_CHANNEL`.
- **`ui.rs`**: Implements the application state machine (`AppState`), menu rendering with `ratatui`, and event handling for navigation.
- **`backlight.rs`**: Defines a static channel for brightness updates (currently unused, reserved for future features).

## Build and Flash Commands

### Prerequisites

1. Install the ESP Rust toolchain: https://esp-rs.github.io/book/installation/index.html
2. Install `probe-rs` for flashing and debugging
3. Connect the device via USB

### Build

```bash
cargo build
```

### Flash and Run

```bash
cargo run
```

This uses the runner defined in `.cargo/config.toml`:
```
probe-rs run --chip=esp32s3 --preverify --always-print-stacktrace --no-location --catch-hardfault
```

### Release Build

```bash
cargo build --release
```

## Development Conventions

### Code Style

- **No standard library**: Uses `#![no_std]` and `#![no_main]`
- **Safety**: Denies `clippy::mem_forget` (unsafe with DMA buffers) and `clippy::large_stack_frames`
- **Comments**: Written in Chinese (项目注释使用中文)
- **Panic handling**: Custom panic handler outputs to RTT

### Async Patterns

- Uses `embassy_executor` for task spawning
- Inter-task communication via `embassy_sync::channel::Channel`
- Input events are sent from `encoder_task` to main loop via `ENCODER_CHANNEL`
- Timeout-based UI refresh: 50ms timeout on channel receive

### Memory Management

- Heap allocator: `esp_alloc` with 128KB heap
- Uses `alloc` for dynamic collections (`Vec`, `String` via `alloc::format!`)
- Stack size threshold enforced by clippy: 1024 bytes

## Architecture Details

### Input Handling

The rotary encoder uses a state machine with a lookup table (`KNOBDIR`) to detect rotation direction. Events are:

- `Clockwise` / `CounterClockwise` - Rotation
- `ConfirmPressed` / `ConfirmReleased` - Encoder push button
- `BackPressed` / `BackReleased` - Back button

### UI State Machine

```
AppState::Menu
  ├── Rotate -> Navigate menu items
  ├── ConfirmReleased -> Select item
  │   └── If Settings -> Enter BrightnessPopup
  └── Back -> (no action currently)

AppState::BrightnessPopup
  ├── Rotate -> Adjust brightness (1-16)
  ├── ConfirmReleased -> Close popup
  └── BackPressed -> Close popup
```

### Display Backend

- Uses `mousefood::EmbeddedBackend` to adapt `ratatui` to embedded displays
- Renders to ST7789 via SPI at 80MHz
- Color order: BGR
- Display offset: (35, 0) - accounts for the display's actual 170×320 visible area vs native resolution

### Backlight Control

Currently implements a simple software PWM by bit-banging GPIO21. The brightness level (1-16) controls how many "off" pulses are sent in a sequence.

## Debugging

- Debug output goes to RTT (Real-Time Transfer)
- View with `probe-rs` or any RTT-compatible debugger
- All major operations print status via `rprintln!`

## Future Expansion Areas

The UI menu includes placeholders for hardware features not yet implemented:

- Wi-Fi (WiFi)
- Bluetooth
- RFID/NFC (PN532)
- Sub-GHz (CC1101)
- IR Remote (红外遥控)

These modules are referenced in the hardware documentation at `ref_doc/T-Embed-CC1101/`.

## Reference Documentation

Hardware documentation is available in the symlinked directory:
- `ref_doc/T-Embed-CC1101/hardware/` - Schematics, datasheets (CC1101, PN532, BQ25896, etc.)
- `ref_doc/T-Embed-CC1101/docs/` - Additional documentation and images
- `ref_doc/T-Embed-CC1101/examples/` - PlatformIO/Arduino example code

## Important Notes

1. **Toolchain**: Must use the Espressif Rust toolchain (`channel = "esp"`)
2. **Flash mode**: DIO (configured in bootloader)
3. **Target triple**: `xtensa-esp32s3-none-elf`
4. **Linker script**: `linkall.x` is required (set in `build.rs`)
5. **Stack protection**: Enabled via `-Z stack-protector=all`

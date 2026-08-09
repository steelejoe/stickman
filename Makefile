# Stickman build targets
#
#   make sim      — desktop simulation (window, Space/mouse input)
#   make device   — ESP32-S3 firmware for T-Display-S3 AMOLED
#   make flash    — build, flash, and monitor the device
#   make import   — convert an image to PNG (sim) + RGB565 (device)

.PHONY: help sim build-sim device build-device flash import clean

CARGO ?= cargo
# Host sim uses stable; device firmware uses the esp toolchain from rust-toolchain.toml.
CARGO_SIM ?= $(CARGO) +stable
SIM_TARGET ?= x86_64-unknown-linux-gnu
DEVICE_TARGET ?= xtensa-esp32s3-none-elf
PYTHON ?= python3

# Display size used by import scaling (landscape AMOLED).
DISPLAY_WIDTH ?= 536
DISPLAY_HEIGHT ?= 240
ASSETS_DIR ?= assets

export PATH := $(HOME)/.cargo/bin:$(PATH)

help:
	@echo "Stickman targets:"
	@echo "  make sim       Build and run desktop simulation"
	@echo "  make build-sim Build simulation binary only"
	@echo "  make device    Build device firmware (release)"
	@echo "  make flash     Flash firmware and open serial monitor"
	@echo "  make import    Convert image → assets/*.png + *.rgb565"
	@echo "                 Usage: make import IMAGE=path/to/img.[png|jpg|jpeg|gif|webp|svg]"
	@echo "                        [NAME=basename] [OUTDIR=assets] [STRETCH_X=1]"
	@echo "  make clean     Remove Cargo build artifacts"

build-sim:
	$(CARGO_SIM) build \
		--bin stickman-sim \
		--no-default-features \
		--features sim \
		--target $(SIM_TARGET)

sim: build-sim
	$(CARGO_SIM) run \
		--bin stickman-sim \
		--no-default-features \
		--features sim \
		--target $(SIM_TARGET)

build-device device:
	@if [ -f "$(HOME)/export-esp.sh" ]; then . "$(HOME)/export-esp.sh"; fi; \
	$(CARGO) build \
		--release \
		--bin stickman \
		--features device \
		--target $(DEVICE_TARGET) \
		-Z build-std=core,alloc

flash:
	@if [ -f "$(HOME)/export-esp.sh" ]; then . "$(HOME)/export-esp.sh"; fi; \
	$(CARGO) run \
		--release \
		--bin stickman \
		--features device \
		--target $(DEVICE_TARGET) \
		-Z build-std=core,alloc

# Convert IMAGE into simulator PNG + device RGB565 under OUTDIR (default: assets).
# Default: fit inside DISPLAY_WIDTH×DISPLAY_HEIGHT, aspect preserved.
# STRETCH_X=1: fit height, then stretch width to fill the display.
#
#   make import IMAGE=photos/sky.jpg NAME=background   # layer-0 backdrop
#   make import IMAGE=icon.svg NAME=foreground
#   make import IMAGE=bg.png STRETCH_X=1
import:
	@if [ -z "$(IMAGE)" ]; then \
		echo "Usage: make import IMAGE=path/to/image.[png|jpg|jpeg|gif|webp|svg] [NAME=basename] [OUTDIR=assets] [STRETCH_X=1]"; \
		exit 1; \
	fi
	$(PYTHON) scripts/import-image.py "$(IMAGE)" \
		--width $(DISPLAY_WIDTH) \
		--height $(DISPLAY_HEIGHT) \
		--out-dir "$(or $(OUTDIR),$(ASSETS_DIR))" \
		$(if $(NAME),--name "$(NAME)",) \
		$(if $(filter 1 yes true YES TRUE,$(STRETCH_X)),--stretch-x,)

clean:
	$(CARGO) clean

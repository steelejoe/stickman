# Stickman build targets
#
#   make sim      — desktop simulation (window, Space/mouse input)
#   make device   — ESP32-S3 firmware for T-Display-S3 AMOLED
#   make flash    — build, flash, and monitor the device

.PHONY: help sim build-sim device build-device flash clean

CARGO ?= cargo
# Host sim uses stable; device firmware uses the esp toolchain from rust-toolchain.toml.
CARGO_SIM ?= $(CARGO) +stable
SIM_TARGET ?= x86_64-unknown-linux-gnu
DEVICE_TARGET ?= xtensa-esp32s3-none-elf

export PATH := $(HOME)/.cargo/bin:$(PATH)

help:
	@echo "Stickman targets:"
	@echo "  make sim       Build and run desktop simulation"
	@echo "  make build-sim Build simulation binary only"
	@echo "  make device    Build device firmware (release)"
	@echo "  make flash     Flash firmware and open serial monitor"
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

clean:
	$(CARGO) clean

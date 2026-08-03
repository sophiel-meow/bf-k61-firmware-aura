##
# Aura
#
# @file
# @version 0.1

TARGET=aura

.PHONY: all build bin flash clean

all: flash

build:
	cargo build --release

devbuild:
	cargo build

bin: build
	cargo objcopy --release -- -O binary $(TARGET).bin

devbin: devbuild
	cargo objcopy -- -O binary $(TARGET)-dev.bin

flash: bin
	./tools/flash-rs/target/release/flash $(TARGET).bin /dev/ttyUSB0

devflash: devbin
	./tools/flash-rs/target/release/flash $(TARGET)-dev.bin /dev/ttyUSB0

clean:
	cargo clean

# end

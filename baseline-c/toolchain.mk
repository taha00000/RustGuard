# arm-none-eabi toolchain config shared by the baseline Makefiles.
# TODO(hardware): verify these match your installed toolchain.
CC      := arm-none-eabi-gcc
OBJCOPY := arm-none-eabi-objcopy
CFLAGS  := -O3 -mcpu=cortex-m4 -mthumb -mfloat-abi=hard -mfpu=fpv4-sp-d16 \
           -ffunction-sections -fdata-sections -Wall

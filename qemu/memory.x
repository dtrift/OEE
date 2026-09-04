/* LM3S6965: 256 KiB flash at 0x00000000, 64 KiB SRAM at 0x20000000.
 * Same memory map as the fork's examples/qemu/memory.x. */
MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 256K
  RAM : ORIGIN = 0x20000000, LENGTH = 64K
}

/* This MCU has no VTOR: real hardware IRQ dispatch is fixed at whatever
 * vector table lives in the first-stage bootloader's own flash region, not
 * at this application's own vector table. The bootloader trampolines a
 * small, fixed set of IRQs through a 10-entry array of raw function
 * pointers ("UserVectors") at a fixed RAM address, 0x20001000..0x20001028
 *  RAM starts just past that block so nothing here ever collides with it.
 */
MEMORY
{
  FLASH : ORIGIN = 0x08002000, LENGTH = 120K
  RAM   : ORIGIN = 0x20001028, LENGTH = 17720
}


//! Minimal PCI configuration space access via I/O ports 0xCF8/0xCFC.
//!
//! Just enough to find a device by vendor/device id and read its I/O
//! BARs and interrupt line; no capability lists, no bridges beyond a
//! plain scan of the first few buses.

use x86_64::instructions::port::Port;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

#[derive(Clone, Copy)]
pub struct PciDevice {
    bus: u8,
    device: u8,
    function: u8,
}

impl PciDevice {
    fn config_read(&self, offset: u8) -> u32 {
        let address = 0x8000_0000
            | (self.bus as u32) << 16
            | (self.device as u32) << 11
            | (self.function as u32) << 8
            | (offset as u32 & 0xFC);
        unsafe {
            Port::<u32>::new(CONFIG_ADDRESS).write(address);
            Port::<u32>::new(CONFIG_DATA).read()
        }
    }

    fn config_write(&self, offset: u8, value: u32) {
        let address = 0x8000_0000
            | (self.bus as u32) << 16
            | (self.device as u32) << 11
            | (self.function as u32) << 8
            | (offset as u32 & 0xFC);
        unsafe {
            Port::<u32>::new(CONFIG_ADDRESS).write(address);
            Port::<u32>::new(CONFIG_DATA).write(value);
        }
    }

    /// Base address of an I/O BAR (bit 0 set); None for memory BARs.
    pub fn io_bar(&self, index: u8) -> Option<u16> {
        let bar = self.config_read(0x10 + index * 4);
        if bar & 1 == 1 {
            Some((bar & !0x3) as u16)
        } else {
            None
        }
    }

    /// The IRQ line the firmware routed this device to.
    pub fn interrupt_line(&self) -> u8 {
        (self.config_read(0x3C) & 0xFF) as u8
    }

    /// Set the I/O space and bus master bits in the command register.
    pub fn enable_io_and_bus_master(&self) {
        let command = self.config_read(0x04);
        self.config_write(0x04, command | 0b101);
    }
}

/// Scan the first buses for a device with the given vendor/device id.
pub fn find(vendor: u16, device: u16) -> Option<PciDevice> {
    for bus in 0..8u8 {
        for dev in 0..32u8 {
            for function in 0..8u8 {
                let candidate = PciDevice {
                    bus,
                    device: dev,
                    function,
                };
                let id = candidate.config_read(0x00);
                if id == 0xFFFF_FFFF {
                    if function == 0 {
                        break; // no device in this slot at all
                    }
                    continue;
                }
                if (id & 0xFFFF) as u16 == vendor && (id >> 16) as u16 == device {
                    return Some(candidate);
                }
                let multifunction = candidate.config_read(0x0C) >> 16 & 0x80 != 0;
                if function == 0 && !multifunction {
                    break;
                }
            }
        }
    }
    None
}

//! Defines the physical address boundaries shared by the CPU, MMU, and image tools.
//! Separates configured memory backing from usable RAM and reserved device space.

pub const ADDRESS_SPACE_SIZE: u32 = 1 << 30;
pub const DEVICE_APERTURE_START: u32 = 0x3ff0_0000;

pub const fn usable_ram_size(backing_size: usize) -> usize {
    if backing_size < DEVICE_APERTURE_START as usize {
        backing_size
    } else {
        DEVICE_APERTURE_START as usize
    }
}

pub const fn is_device_aperture(address: u32) -> bool {
    address >= DEVICE_APERTURE_START && address < ADDRESS_SPACE_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aperture_is_reserved_independently_of_configured_backing() {
        for size in [
            0,
            8,
            65536,
            1 << 20,
            1 << 29,
            DEVICE_APERTURE_START as usize,
        ] {
            assert_eq!(usable_ram_size(size), size);
        }
        assert_eq!(usable_ram_size(1 << 30), 0x3ff0_0000);
        assert!(!is_device_aperture(DEVICE_APERTURE_START - 1));
        assert!(is_device_aperture(DEVICE_APERTURE_START));
        assert!(is_device_aperture(ADDRESS_SPACE_SIZE - 1));
        assert!(!is_device_aperture(ADDRESS_SPACE_SIZE));
    }
}

use error::RibosomeReturnCode;

//--------------------------------------------------------------------------------------------------
// Helpers
//--------------------------------------------------------------------------------------------------

const U16_MAX: u32 = <u16>::max_value() as u32;

/// returns the u16 high bits from a u32
pub fn u32_high_bits(i: u32) -> u16 {
    (i >> 16) as u16
}

/// returns the u16 low bits from a u32
pub fn u32_low_bits(i: u32) -> u16 {
    (i as u16 % <u16>::max_value())
}

/// splits the high and low bits of u32 into a tuple of u16, for destructuring convenience
pub fn u32_split_bits(i: u32) -> (u16, u16) {
    (u32_high_bits(i), u32_low_bits(i))
}

/// merges 2x u16 into a single u32
pub fn u32_merge_bits(high: u16, low: u16) -> u32 {
    (u32::from(high) << 16) | u32::from(low)
}

//--------------------------------------------------------------------------------------------------
// Single Page Memory Allocation
//--------------------------------------------------------------------------------------------------

#[derive(Copy, Clone, Debug)]
/// SinglePageAllocation is a memory allocation garanteed to fit in a WASM 64KiB Memory Page
pub struct SinglePageAllocation {
    pub offset: u16,
    pub length: u16,
}

#[allow(unknown_lints)]
#[allow(cast_lossless)]
impl SinglePageAllocation {
    /// An Encoded Allocation is a u32 where 'offset' is first 16-bits and 'length' last 16-bits
    /// A valid allocation must not have a length of zero
    /// An Encoded Allocation with an offset but no length is actually an encoding of an ErrorCode
    pub fn new(encoded_allocation: u32) -> Result<Self, RibosomeReturnCode> {
        let (offset, length) = u32_split_bits(encoded_allocation);
        let allocation = SinglePageAllocation { offset, length };

        // zero length allocation = encoding an error api return code
        if allocation.length == 0 {
            // @TODO is it right to return success as Err for 0? what is a "success" error?
            // @see https://github.com/holochain/holochain-rust/issues/181
            return Err(RibosomeReturnCode::from_offset(allocation.offset));
        }

        // should never happen
        // we don't panic because this needs to work with wasm, which doesn't support panic
        if (allocation.offset as u32 + allocation.length as u32) > U16_MAX {
            return Err(RibosomeReturnCode::OutOfMemory);
        }

        Ok(allocation)
    }

    /// returns a single u32 value encoding both the u16 offset and length values
    pub fn encode(self) -> u32 {
        u32_merge_bits(self.offset, self.length)
    }
}

//--------------------------------------------------------------------------------------------------
// Single Page Memory Stack Manager
//--------------------------------------------------------------------------------------------------

#[derive(Copy, Clone, Default, Debug)]
/// Struct for managing a WASM 64KiB memory page as a stack
pub struct SinglePageStack {
    top: u16,
}

#[allow(unknown_lints)]
#[allow(cast_lossless)]
impl SinglePageStack {
    // A stack can be initialized by giving the last know allocation on this stack
    pub fn new(last_allocation: SinglePageAllocation) -> Self {
        assert!(last_allocation.offset as u32 + last_allocation.length as u32 <= U16_MAX);
        SinglePageStack {
            top: last_allocation.offset + last_allocation.length,
        }
    }

    pub fn from_encoded(encoded_last_allocation: u32) -> Self {
        let last_allocation = SinglePageAllocation::new(encoded_last_allocation as u32);
        let last_allocation =
            last_allocation.expect("received error instead of valid encoded allocation");
        assert!(last_allocation.offset as u32 + last_allocation.length as u32 <= U16_MAX);
        return SinglePageStack::new(last_allocation);
    }

    pub fn allocate(&mut self, size: u16) -> u16 {
        assert!(self.top as u32 + size as u32 <= U16_MAX);
        let offset = self.top;
        self.top += size;
        offset
    }

    pub fn deallocate(&mut self, allocation: SinglePageAllocation) -> Result<(), ()> {
        if self.top == allocation.offset + allocation.length {
            self.top = allocation.offset;
            return Ok(());
        }
        Err(())
    }

    // Getters
    pub fn top(self) -> u16 {
        self.top
    }
}

#[cfg(test)]
pub mod tests {

    use super::*;
    use error::RibosomeReturnCode;

    #[test]
    /// tests construction and encoding in a new single page allocation
    fn new_spa() {
        let i = 0b1010101010101010_0101010101010101;
        let spa = SinglePageAllocation::new(i).unwrap();

        assert_eq!(0b1010101010101010, spa.offset);

        assert_eq!(0b0101010101010101, spa.length);
    }

    #[test]
    /// tests that we can encode error return codes (zero length allocation)
    fn new_spa_error() {
        assert_eq!(
            // offset 0 = success?
            // @see https://github.com/holochain/holochain-rust/issues/181
            SinglePageAllocation::new(0b0000000000000000_0000000000000000).unwrap_err(),
            RibosomeReturnCode::Success,
        );

        assert_eq!(
            // offset 1 = generic error
            SinglePageAllocation::new(0b0000000000000001_0000000000000000).unwrap_err(),
            RibosomeReturnCode::Failure,
        );

        assert_eq!(
            // offset 2 = serde json error
            SinglePageAllocation::new(0b0000000000000010_0000000000000000).unwrap_err(),
            RibosomeReturnCode::ArgumentDeserializationFailed,
        );

        assert_eq!(
            // offset 3 = page overflow error
            SinglePageAllocation::new(0b0000000000000011_0000000000000000).unwrap_err(),
            RibosomeReturnCode::OutOfMemory,
        );

        assert_eq!(
            // offset 4 = page overflow error
            SinglePageAllocation::new(0b0000000000000100_0000000000000000).unwrap_err(),
            RibosomeReturnCode::ReceivedWrongActionResult,
        );

        assert_eq!(
            // nonsense offset = generic error
            SinglePageAllocation::new(0b1010101010101010_0000000000000000).unwrap_err(),
            RibosomeReturnCode::Failure,
        );
    }

    #[test]
    /// tests that a SinglePageAllocation returns its encoded offset/length pair as u32
    fn spa_encode() {
        let i = 0b1010101010101010_0101010101010101;
        let spa = SinglePageAllocation::new(i).unwrap();

        assert_eq!(i, spa.encode());
    }

    #[test]
    /// tests that we can extract the high bits from a u32 into the correct u16
    fn u32_high_bits() {
        assert_eq!(
            0b1010101010101010,
            super::u32_high_bits(0b1010101010101010_0101010101010101),
        );
    }

    #[test]
    /// tests that we can extract the high bits from a u32 into the correct u16
    fn u32_low_bits() {
        assert_eq!(
            0b0101010101010101,
            super::u32_low_bits(0b1010101010101010_0101010101010101),
        );
    }

    #[test]
    /// tests that we can split a u32 into a tuple of high/low bits
    fn u32_split_bits() {
        assert_eq!(
            (0b1010101010101010, 0b0101010101010101),
            super::u32_split_bits(0b1010101010101010_0101010101010101),
        );
    }

    #[test]
    /// tests that we can merge a u16 tuple into a u32
    fn u32_merge_bits() {
        assert_eq!(
            0b1010101010101010_0101010101010101,
            super::u32_merge_bits(0b1010101010101010, 0b0101010101010101),
        );
    }

}

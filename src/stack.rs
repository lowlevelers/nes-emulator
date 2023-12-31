use anyhow::Result;

use crate::constant::SP_BASE_ADDRESS;

// stack manager
pub trait Stacked {
    /// push a byte to stack
    fn push_stack(&mut self, val: u8) -> Result<()>;
    /// pull a byte from stack
    fn pop_stack(&mut self) -> Result<u8>;

    fn push_stack16(&mut self, val: u16) -> Result<()> {
        let (hi, lo) = (((val & 0xFF00) >> 8) as u8, (val & 0xFF) as u8);
        self.push_stack(hi)?;
        self.push_stack(lo)?;
        Ok(())
    }

    fn pop_stack16(&mut self) -> Result<u16> {
        let lo = self.pop_stack()? as u16;
        let hi = self.pop_stack()? as u16;
        let res = (hi << 8) | lo;
        Ok(res)
    }
}

// get stack pointer address with offset
pub fn get_sp_offset(sp: u8) -> u16 {
    SP_BASE_ADDRESS | u16::from(sp)
}

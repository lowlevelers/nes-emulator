use anyhow::{Ok, Result};

pub trait Mem {
    // Read data from memory
    fn mem_read(&self, addr: u16) -> Result<u8>;
    // Write data to memory
    fn mem_write(&mut self, addr: u16, data: u8) -> Result<()>;

    // Read 16-bit data from the memory
    // Because our memory stores u8, we must use arithmetic in Rust to convert ot u16
    fn mem_read_u16(&self, pos: u16) -> Result<u16> {
        let lo = self.mem_read(pos)? as u16;
        let hi = self.mem_read(pos + 1)? as u16;
        Ok((hi << 8) | lo)
    }
    // Write 16-bit data to the u8 alginment memory
    fn mem_write_u16(&mut self, pos: u16, data: u16) -> Result<()> {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo)?;
        self.mem_write(pos + 1, hi)?;
        Ok(())
    }
}

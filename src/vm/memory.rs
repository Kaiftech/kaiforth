use memmap2::{MmapMut, MmapOptions};
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

pub struct Memory { 
    pub raw: MmapMut, 
    pub here: usize 
}

impl Memory {
    pub fn try_new(sz: usize) -> ForthResult<Self> {
        let raw = MmapOptions::new().len(sz).map_anon()
            .map_err(|e| ForthError::new(ForthErrorKind::FileError { context: "MmapAnon".into(), source: e.to_string() }, ForthPhase::Execution, "Failed to map memory for VM"))?;
        Ok(Self { raw, here: 0 })
    }
    
    #[inline(always)] pub fn read_u8(&self, a: usize) -> ForthResult<u8> {
        if a >= self.raw.len() { return Err(ForthError::new(ForthErrorKind::MemoryOOB{addr:a, limit:self.raw.len()}, ForthPhase::Execution, "OOB u8 Read")); }
        Ok(self.raw[a])
    }
    #[inline(always)] pub fn write_u8(&mut self, a: usize, v: u8) -> ForthResult<()> {
        if a >= self.raw.len() { return Err(ForthError::new(ForthErrorKind::MemoryOOB{addr:a, limit:self.raw.len()}, ForthPhase::Execution, "OOB u8 Write")); }
        self.raw[a] = v; Ok(())
    }
    #[inline(always)] pub fn read_i64(&self, a: usize) -> ForthResult<i64> {
        if a + 8 > self.raw.len() { return Err(ForthError::new(ForthErrorKind::MemoryOOB{addr:a, limit:self.raw.len()}, ForthPhase::Execution, "OOB i64 Read")); }
        let mut b = [0u8; 8]; b.copy_from_slice(&self.raw[a..a+8]);
        Ok(i64::from_le_bytes(b))
    }
    #[inline(always)] pub fn write_i64(&mut self, a: usize, v: i64) -> ForthResult<()> {
        if a + 8 > self.raw.len() { return Err(ForthError::new(ForthErrorKind::MemoryOOB{addr:a, limit:self.raw.len()}, ForthPhase::Execution, "OOB i64 Write")); }
        self.raw[a..a+8].copy_from_slice(&v.to_le_bytes()); Ok(())
    }
    pub fn allot(&mut self, n: usize) -> ForthResult<usize> {
        let pad = (8 - (self.here % 8)) % 8;
        let a = self.here + pad; 
        if a + n > self.raw.len() { return Err(ForthError::new(ForthErrorKind::MemoryOOB { addr: a + n, limit: self.raw.len() }, ForthPhase::Execution, "Allot OOB")); }
        self.here = a + n; Ok(a)
    }
}

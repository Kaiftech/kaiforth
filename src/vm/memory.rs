use memmap2::{MmapMut, MmapOptions};
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

pub struct Memory { 
    raw: MmapMut, 
    pub here: usize 
}

impl Memory {
    pub fn try_new(sz: usize) -> ForthResult<Self> {
        let raw = MmapOptions::new().len(sz).map_anon()
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution))?;
        Ok(Self { raw, here: 0 })
    }

    pub fn try_clone(&self) -> ForthResult<Self> {
        let mut new_raw = MmapOptions::new().len(self.raw.len()).map_anon()
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution))?;
        new_raw.copy_from_slice(&self.raw);
        Ok(Self { raw: new_raw, here: self.here })
    }
    
    #[inline(always)] 
    pub fn read_u8(&self, a: usize) -> ForthResult<u8> {
        if a >= self.raw.len() { 
            return Err(ForthError::new(ForthErrorKind::MemoryOOB { addr: a, limit: self.raw.len() }, ForthPhase::Execution)); 
        }
        Ok(self.raw[a])
    }

    #[inline(always)] 
    pub fn write_u8(&mut self, a: usize, v: u8) -> ForthResult<()> {
        if a >= self.raw.len() { 
            return Err(ForthError::new(ForthErrorKind::MemoryOOB { addr: a, limit: self.raw.len() }, ForthPhase::Execution)); 
        }
        self.raw[a] = v; Ok(())
    }

    #[inline(always)] 
    pub fn read_i64(&self, a: usize) -> ForthResult<i64> {
        if a + 8 > self.raw.len() { 
            return Err(ForthError::new(ForthErrorKind::MemoryOOB { addr: a, limit: self.raw.len() }, ForthPhase::Execution)); 
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.raw[a..a+8]);
        Ok(i64::from_le_bytes(b))
    }

    #[inline(always)] 
    pub fn write_i64(&mut self, a: usize, v: i64) -> ForthResult<()> {
        if a + 8 > self.raw.len() { 
            return Err(ForthError::new(ForthErrorKind::MemoryOOB { addr: a, limit: self.raw.len() }, ForthPhase::Execution)); 
        }
        self.raw[a..a+8].copy_from_slice(&v.to_le_bytes()); 
        Ok(())
    }

    pub fn allot(&mut self, n: usize) -> ForthResult<usize> {
        let pad = (8 - (self.here % 8)) % 8;
        let a = self.here + pad; 
        if a + n > self.raw.len() { 
            return Err(ForthError::new(ForthErrorKind::MemoryOOB { addr: a + n, limit: self.raw.len() }, ForthPhase::Execution)); 
        }
        self.here = a + n; 
        Ok(a)
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.raw.as_mut_ptr()
    }

    pub fn get_raw_slice(&self) -> &[u8] {
        &self.raw
    }

    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        &mut self.raw
    }

    pub fn len(&self) -> usize {
        self.raw.len()
    }
}


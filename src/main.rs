use std::collections::HashMap;
use std::io::{Write, stdout, Read};
use std::time::Instant;

#[derive(Debug, PartialEq, Clone)]
pub enum ForthErrorKind {
    StackUnderflow { exp: usize, found: usize },
    DivisionByZero,
    UnknownToken(String),
    MemoryOOB { addr: usize, limit: usize },
    UnmatchedControlFlow,
    InvalidOpcode(u8),
    EndOfCode,
    WordNotFound(String),
}

impl std::fmt::Display for ForthErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StackUnderflow { exp, found } => write!(f, "Stack underflow (exp {}, found {})", exp, found),
            Self::DivisionByZero => write!(f, "Division by zero"),
            Self::UnknownToken(s) => write!(f, "Unknown token: '{}'", s),
            Self::MemoryOOB { addr, limit } => write!(f, "Mem OOB: 0x{:X} (lim 0x{:X})", addr, limit),
            Self::UnmatchedControlFlow => write!(f, "Unmatched control flow"),
            Self::InvalidOpcode(o) => write!(f, "Invalid opcode: 0x{:X}", o),
            Self::EndOfCode => write!(f, "Unexpected end of code"),
            Self::WordNotFound(s) => write!(f, "Word not found: '{}'", s),
        }
    }
}

pub struct RichError {
    pub kind: ForthErrorKind,
    pub line: usize,
    pub col: usize,
    pub stack: Vec<i64>,
    pub f_stack: Vec<f64>,
    pub word: Option<String>,
    pub call_stack: Vec<String>,
}

impl std::fmt::Display for RichError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n\x1b[1;31m[!] FORTH EXCEPTION\x1b[0m")?;
        writeln!(f, "\x1b[1mReason:\x1b[0m {}", self.kind)?;
        writeln!(f, "\x1b[1mLoc:\x1b[0m    Line {}, Col {}", self.line, self.col)?;
        if let Some(ref w) = self.word { writeln!(f, "  \x1b[1mWord:\x1b[0m   {}", w)?; }
        writeln!(f, "  \x1b[1mStacks:\x1b[0m  Data: {:?} | Float: {:?}", self.stack, self.f_stack)?;
        if !self.call_stack.is_empty() { writeln!(f, "  \x1b[1mCalls:\x1b[0m   {}", self.call_stack.join(" → "))?; }
        Ok(())
    }
}

pub type ForthResult<T> = Result<T, ForthErrorKind>;
type PrimitiveFn = fn(&mut Vm) -> ForthResult<()>;

#[repr(u8)] #[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Noop = 0, Push = 1, PushF = 2, Prim = 3, Call = 4, Exit = 5, Jump = 6, JZ = 7,
    Square = 8, Nip = 9, Tuck = 10, Inc = 11, Dec = 12, IsZero = 13, Drop2 = 14, Dup2 = 15,
}

impl Op {
    fn from_u8(v: u8) -> ForthResult<Self> {
        match v {
            0 => Ok(Op::Noop), 1 => Ok(Op::Push), 2 => Ok(Op::PushF), 3 => Ok(Op::Prim),
            4 => Ok(Op::Call), 5 => Ok(Op::Exit), 6 => Ok(Op::Jump), 7 => Ok(Op::JZ),
            8 => Ok(Op::Square), 9 => Ok(Op::Nip), 10 => Ok(Op::Tuck), 11 => Ok(Op::Inc),
            12 => Ok(Op::Dec), 13 => Ok(Op::IsZero), 14 => Ok(Op::Drop2), 15 => Ok(Op::Dup2),
            _ => Err(ForthErrorKind::InvalidOpcode(v)),
        }
    }
}

pub struct Memory { raw: Vec<u8> }
impl Memory {
    fn new(sz: usize) -> Self { Self { raw: vec![0; sz] } }
    #[inline(always)] fn read_i64(&self, a: usize) -> ForthResult<i64> {
        if a + 8 > self.raw.len() { return Err(ForthErrorKind::MemoryOOB { addr: a, limit: self.raw.len() }); }
        let bytes = self.raw[a..a+8].try_into().map_err(|_| ForthErrorKind::MemoryOOB { addr: a, limit: self.raw.len() })?;
        Ok(i64::from_le_bytes(bytes))
    }
    #[inline(always)] fn write_i64(&mut self, a: usize, v: i64) -> ForthResult<()> {
        if a + 8 > self.raw.len() { return Err(ForthErrorKind::MemoryOOB { addr: a, limit: self.raw.len() }); }
        self.raw[a..a+8].copy_from_slice(&v.to_le_bytes()); Ok(())
    }
    fn allot(&mut self, n: usize) -> usize { let a = self.raw.len(); let pad = (8 - (a % 8)) % 8; self.raw.extend(vec![0; pad + n]); a + pad }
}

pub struct CodeBuf { ops: Vec<u8>, data: Vec<u64>, src: Vec<(usize, usize)> }
impl CodeBuf {
    fn new() -> Self { Self { ops: Vec::new(), data: Vec::new(), src: Vec::new() } }
    fn push(&mut self, op: Op, val: u64, l: usize, c: usize) { self.ops.push(op as u8); self.data.push(val); self.src.push((l, c)); }
    fn clear(&mut self) { self.ops.clear(); self.data.clear(); self.src.clear(); }
}

#[derive(Clone, Copy)] pub enum WordKind { Prim(PrimitiveFn), Defined(usize), Var(usize), Const(i64) }
pub struct WordEntry { name: String, kind: WordKind, immediate: bool }

pub struct Vm {
    dict: Vec<WordEntry>, lookup: HashMap<String, usize>,
    memory: Memory, d_stack: Vec<i64>, f_stack: Vec<f64>, c_stack: Vec<Frame>,
    code: CodeBuf, tr_code: CodeBuf,
    compiling: bool,
    // Control-flow patch stacks: if/else/then use if_stack; begin/until uses loop_stack
    if_stack: Vec<usize>, loop_stack: Vec<usize>,
    tracing: bool, bench: bool,
}

#[derive(Clone, Copy)] pub struct Frame { pub ret_ip: usize, pub word_idx: usize, pub ret_in_tr: bool }

impl Vm {
    pub fn new() -> Self {
        let mut vm = Self { dict: Vec::new(), lookup: HashMap::new(), memory: Memory::new(65536), d_stack: Vec::with_capacity(32), f_stack: Vec::new(), c_stack: Vec::new(), code: CodeBuf::new(), tr_code: CodeBuf::new(), compiling: false, if_stack: Vec::new(), loop_stack: Vec::new(), tracing: false, bench: false };
        vm.init(); vm
    }

    fn init(&mut self) {
        let p: [(&str, PrimitiveFn, bool); 37] = [
            ("+", Vm::add, false), ("-", Vm::sub, false), ("*", Vm::mul, false), ("/", Vm::div, false),
            ("dup", Vm::dup, false), ("drop", Vm::drop, false), ("swap", Vm::swap, false), ("over", Vm::over, false),
            (".", Vm::dot, false), (".s", Vm::dot_s, false), ("emit", Vm::emit_p, false), ("cr", Vm::cr, false),
            ("f+", Vm::fadd, false), ("f-", Vm::fsub, false), ("f*", Vm::fmul, false), ("f/", Vm::fdiv, false), ("f.", Vm::fdot, false),
            ("@", Vm::fetch, false), ("!", Vm::store, false),
            ("see", Vm::see, true), ("trace", Vm::trace, false), ("bench", Vm::bench, false),
            ("immediate", Vm::imm_p, false), ("bye", Vm::bye, false),
            ("2dup", Vm::dup2, false), ("2drop", Vm::drop2, false),
            ("<", Vm::lt, false), (">", Vm::gt, false), ("=", Vm::eq, false),
            ("0=", Vm::zero_eq, false), ("0<", Vm::zero_lt, false),
            ("negate", Vm::negate, false), ("abs", Vm::abs_v, false), ("mod", Vm::mod_v, false),
            ("max", Vm::max_v, false), ("min", Vm::min_v, false),
            ("include", Vm::include_p, false),
        ];
        for (n, f, imm) in p { let idx = self.dict.len(); self.dict.push(WordEntry { name: n.to_string(), kind: WordKind::Prim(f), immediate: imm }); self.lookup.insert(n.to_string(), idx); }
    }

    pub fn eval(&mut self, input: &str) -> Result<(), RichError> {
        let mut line = 1;
        for line_str in input.lines() {
            let mut col = 1; let mut words = line_str.split_whitespace().peekable();
            while let Some(w) = words.next() {
                // Handle 'include <path>' at the eval level for real file loading
                if w == "include" {
                    let path = words.next().ok_or_else(|| self.rich_err(ForthErrorKind::UnknownToken("(expected filename)".into()), line, col))?;
                    let src = std::fs::read_to_string(path).map_err(|e| self.rich_err(ForthErrorKind::UnknownToken(e.to_string()), line, col))?;
                    self.eval(&src)?;
                    continue;
                }
                if let Err(e) = self.compile_one(w, line, col, &mut words) { return Err(self.rich_err(e, line, col)); }
                col += w.len() + 1;
            }
            line += 1;
        }
        if !self.compiling && !self.tr_code.ops.is_empty() {
            self.optimize(true); self.tr_code.push(Op::Exit, 0, 0, 0);
            let start = Instant::now(); let res = self.run_loop(0, true);
            if self.bench { println!("\x1b[1;30m(Time: {:?})\x1b[0m", start.elapsed()); }
            self.tr_code.clear(); res.map_err(|e| self.rich_err(e, 0, 0))?;
        }
        Ok(())
    }

    fn compile_one(&mut self, t: &str, l: usize, c: usize, rest: &mut std::iter::Peekable<std::str::SplitWhitespace>) -> ForthResult<()> {
        // Handle standard Forth comments
        if t == "(" { while let Some(w) = rest.next() { if w.ends_with(')') { break; } } return Ok(()); }
        if t == "\\" { while rest.next().is_some() {} return Ok(()); }
        if let Some(&idx) = self.lookup.get(t) {
            if self.dict[idx].immediate {
                if t == "see" { let next = rest.next().ok_or(ForthErrorKind::UnknownToken("(expected word name)".into()))?; self.see_word(next)?; return Ok(()); }
                let old = std::mem::replace(&mut self.tr_code, CodeBuf::new());
                self.emit_word(idx, l, c, true); self.tr_code.push(Op::Exit, 0, 0, 0); self.run_loop(0, true)?;
                self.tr_code = old; return Ok(());
            }
        }
        match t {
            ":" => { let n = rest.next().ok_or(ForthErrorKind::UnknownToken("(expected word name)".into()))?; let idx = self.dict.len(); self.dict.push(WordEntry { name: n.to_string(), kind: WordKind::Defined(self.code.ops.len()), immediate: false }); self.lookup.insert(n.to_string(), idx); self.compiling = true; }
            ";" => { self.emit_instr(Op::Exit, 0, l, c, false); self.optimize(false); self.compiling = false; }
            "recurse" => { if !self.compiling { return Err(ForthErrorKind::UnmatchedControlFlow); } let idx = self.dict.len() - 1; self.emit_word(idx, l, c, false); }
            // if ... [else] ... then
            "if" => { let idx = self.buf_m().ops.len(); self.if_stack.push(idx); self.emit_instr(Op::JZ, 0, l, c, false); }
            "else" => {
                let jz_idx = self.if_stack.pop().ok_or(ForthErrorKind::UnmatchedControlFlow)?;
                // Emit unconditional jump past the else-branch; patch later via then
                let jump_idx = self.buf_m().ops.len(); self.if_stack.push(jump_idx); self.emit_instr(Op::Jump, 0, l, c, false);
                // Patch the JZ to land here (start of else-branch)
                let here = self.buf_m().ops.len(); self.buf_m().data[jz_idx] = here as u64;
            }
            "then" => { let idx = self.if_stack.pop().ok_or(ForthErrorKind::UnmatchedControlFlow)?; let cur = self.buf_m().ops.len(); self.buf_m().data[idx] = cur as u64; }
            // begin ... until  (loops while top-of-stack is 0, exits when nonzero)
            "begin" => { let idx = self.buf_m().ops.len(); self.loop_stack.push(idx); }
            "until" => { let begin_idx = self.loop_stack.pop().ok_or(ForthErrorKind::UnmatchedControlFlow)?; self.emit_instr(Op::JZ, begin_idx as u64, l, c, false); }
            // begin ... while ... repeat  (conditional loop)
            "while" => { let idx = self.buf_m().ops.len(); self.if_stack.push(idx); self.emit_instr(Op::JZ, 0, l, c, false); }
            "repeat" => {
                let begin_idx = self.loop_stack.pop().ok_or(ForthErrorKind::UnmatchedControlFlow)?;
                self.emit_instr(Op::Jump, begin_idx as u64, l, c, false);
                let jz_idx = self.if_stack.pop().ok_or(ForthErrorKind::UnmatchedControlFlow)?;
                let here = self.buf_m().ops.len(); self.buf_m().data[jz_idx] = here as u64;
            }
            "variable" => { let n = rest.next().ok_or(ForthErrorKind::UnknownToken("(expected variable name)".into()))?; let a = self.memory.allot(8); let idx = self.dict.len(); self.dict.push(WordEntry { name: n.to_string(), kind: WordKind::Var(a), immediate: false }); self.lookup.insert(n.to_string(), idx); }
            "constant" => { let n = rest.next().ok_or(ForthErrorKind::UnknownToken("(expected constant name)".into()))?; let v = self.pop().map_err(|e| e)?; let idx = self.dict.len(); self.dict.push(WordEntry { name: n.to_string(), kind: WordKind::Const(v), immediate: false }); self.lookup.insert(n.to_string(), idx); }
            "immediate" => { if let Some(w) = self.dict.last_mut() { w.immediate = true; } }
            "exit" => { self.emit_instr(Op::Exit, 0, l, c, false); }
            _ => {
                if let Some(&idx) = self.lookup.get(t) { self.emit_word(idx, l, c, false); }
                else if let Ok(v) = t.parse::<i64>() { self.emit_instr(Op::Push, v as u64, l, c, false); }
                else if let Ok(v) = t.parse::<f64>() { self.emit_instr(Op::PushF, v.to_bits(), l, c, false); }
                else { return Err(ForthErrorKind::UnknownToken(t.to_string())); }
            }
        }
        Ok(())
    }

    fn buf_m(&mut self) -> &mut CodeBuf { if self.compiling { &mut self.code } else { &mut self.tr_code } }
    fn emit_instr(&mut self, op: Op, val: u64, l: usize, c: usize, tr: bool) { if self.compiling && !tr { self.code.push(op, val, l, c); } else { self.tr_code.push(op, val, l, c); } }
    fn emit_word(&mut self, idx: usize, l: usize, c: usize, tr: bool) { match self.dict[idx].kind { WordKind::Prim(_) => self.emit_instr(Op::Prim, idx as u64, l, c, tr), _ => self.emit_instr(Op::Call, idx as u64, l, c, tr) } }

    fn optimize(&mut self, tr: bool) {
        loop {
            let mut changed = false; let buf = if tr { &mut self.tr_code } else { &mut self.code }; let len = buf.ops.len();
            for i in 0..len {
                if buf.ops[i] == Op::Noop as u8 { continue; }
                if i + 1 < len && buf.ops[i] == Op::Push as u8 {
                    let v1 = buf.data[i] as i64;
                    if buf.ops[i+1] == Op::Push as u8 {
                        let v2 = buf.data[i+1] as i64;
                        if i + 2 < len && buf.ops[i+2] == Op::Prim as u8 {
                            let p_idx = buf.data[i+2] as usize; let name = &self.dict[p_idx].name;
                            let res = match name.as_str() { "+" => Some(v1.wrapping_add(v2)), "-" => Some(v1.wrapping_sub(v2)), "*" => Some(v1.wrapping_mul(v2)), "/" if v2 != 0 => Some(v1 / v2), _ => None };
                            if let Some(r) = res { buf.data[i] = r as u64; buf.ops[i+1] = Op::Noop as u8; buf.ops[i+2] = Op::Noop as u8; changed = true; }
                        }
                    } else {
                        if let Ok(next) = Op::from_u8(buf.ops[i+1]) {
                            let res = match next { Op::Square => Some(v1.wrapping_mul(v1)), Op::Inc => Some(v1.wrapping_add(1)), Op::Dec => Some(v1.wrapping_sub(1)), Op::IsZero => Some(if v1 == 0 { -1 } else { 0 }), _ => None };
                            if let Some(r) = res { buf.data[i] = r as u64; buf.ops[i+1] = Op::Noop as u8; changed = true; }
                        }
                    }
                }
                if i + 1 < len && buf.ops[i] == Op::PushF as u8 && buf.ops[i+1] == Op::PushF as u8 {
                    let (v1, v2) = (f64::from_bits(buf.data[i]), f64::from_bits(buf.data[i+1]));
                    if i + 2 < len && buf.ops[i+2] == Op::Prim as u8 {
                        let p_idx = buf.data[i+2] as usize; let name = &self.dict[p_idx].name;
                        let res = match name.as_str() { "f+" => Some(v1 + v2), "f-" => Some(v1 - v2), "f*" => Some(v1 * v2), "f/" if v2 != 0.0 => Some(v1 / v2), _ => None };
                        if let Some(r) = res { buf.data[i] = r.to_bits(); buf.ops[i+1] = Op::Noop as u8; buf.ops[i+2] = Op::Noop as u8; changed = true; }
                    }
                }
                if i + 1 < len && buf.ops[i] == Op::Prim as u8 && buf.ops[i+1] == Op::Prim as u8 {
                    let (n1, n2) = (&self.dict[buf.data[i] as usize].name, &self.dict[buf.data[i+1] as usize].name);
                    let si = match (n1.as_str(), n2.as_str()) { ("dup", "*") => Some(Op::Square), ("swap", "drop") => Some(Op::Nip), ("over", "+") => Some(Op::Tuck), ("dup", "dup") => Some(Op::Dup2), ("drop", "drop") => Some(Op::Drop2), _ => None };
                    if let Some(s) = si { buf.ops[i] = s as u8; buf.ops[i+1] = Op::Noop as u8; changed = true; }
                }
            }
            let mut j = 0; while j < buf.ops.len() { if buf.ops[j] == Op::Noop as u8 { buf.ops.remove(j); buf.data.remove(j); buf.src.remove(j); } else { j += 1; } }
            if !changed { break; }
        }
    }

    #[inline(always)]
    fn run_loop(&mut self, mut ip: usize, mut in_tr: bool) -> ForthResult<()> {
        loop {
            let buf = if in_tr { &self.tr_code } else { &self.code };
            if ip >= buf.ops.len() { return Err(ForthErrorKind::EndOfCode); }
            let op = Op::from_u8(buf.ops[ip])?; let data = buf.data[ip];
            if self.tracing {
                let old = self.d_stack.clone(); let (l, c) = buf.src[ip];
                print!("\x1b[34m[TRACE]\x1b[0m {:4} | {:10} | {:>4}:{:>2} | ", ip, format!("{:?}", op), l, c);
                self.step_ex(op, data, &mut ip, &mut in_tr)?;
                println!("\x1b[32m{:?} → {:?}\x1b[0m", old, self.d_stack);
            } else { self.step_ex(op, data, &mut ip, &mut in_tr)?; }
            if ip == usize::MAX { break; }
        }
        Ok(())
    }

    #[inline(always)]
    fn step_ex(&mut self, op: Op, data: u64, ip: &mut usize, in_tr: &mut bool) -> ForthResult<()> {
        match op {
            Op::Push => { self.d_stack.push(data as i64); *ip += 1; Ok(()) }
            Op::PushF => { self.f_stack.push(f64::from_bits(data)); *ip += 1; Ok(()) }
            Op::Prim => { let idx = data as usize; if let WordKind::Prim(f) = self.dict[idx].kind { *ip += 1; f(self) } else { unreachable!() } }
            Op::Call => {
                let idx = data as usize;
                match self.dict[idx].kind {
                    WordKind::Defined(target) => {
                        self.c_stack.push(Frame { ret_ip: *ip + 1, word_idx: idx, ret_in_tr: *in_tr });
                        *ip = target; *in_tr = false; Ok(())
                    }
                    WordKind::Var(a) => { self.d_stack.push(a as i64); *ip += 1; Ok(()) }
                    WordKind::Const(v) => { self.d_stack.push(v); *ip += 1; Ok(()) }
                    _ => unreachable!(),
                }
            }
            Op::Exit => {
                if let Some(f) = self.c_stack.pop() {
                    *ip = f.ret_ip; *in_tr = f.ret_in_tr; Ok(())
                } else { *ip = usize::MAX; Ok(()) }
            }
            Op::Square => { let v = self.pop()?; self.d_stack.push(v.wrapping_mul(v)); *ip += 1; Ok(()) }
            Op::Inc => { let v = self.pop()?; self.d_stack.push(v.wrapping_add(1)); *ip += 1; Ok(()) }
            Op::Dec => { let v = self.pop()?; self.d_stack.push(v.wrapping_sub(1)); *ip += 1; Ok(()) }
            Op::Nip => { let (a, _) = self.pop2()?; self.d_stack.push(a); *ip += 1; Ok(()) }
            Op::Tuck => { let (a, b) = self.pop2()?; self.d_stack.push(b); self.d_stack.push(a); self.d_stack.push(b); *ip += 1; Ok(()) }
            Op::JZ => { let v = self.pop()?; *ip = if v == 0 { data as usize } else { *ip + 1 }; Ok(()) }
            Op::Jump => { *ip = data as usize; Ok(()) }
            Op::Noop => { *ip += 1; Ok(()) }
            Op::IsZero => { let v = self.pop()?; self.d_stack.push(if v == 0 { -1 } else { 0 }); *ip += 1; Ok(()) }
            Op::Drop2 => { self.pop2()?; *ip += 1; Ok(()) }
            Op::Dup2 => { let (a, b) = self.pop2()?; self.d_stack.push(a); self.d_stack.push(b); self.d_stack.push(a); self.d_stack.push(b); *ip += 1; Ok(()) }
        }
    }


    fn pop(&mut self) -> ForthResult<i64> { self.d_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 1, found: 0 }) }
    fn pop2(&mut self) -> ForthResult<(i64, i64)> { let b = self.pop()?; let a = self.d_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 2, found: 1 })?; Ok((a, b)) }
    fn rich_err(&self, k: ForthErrorKind, l: usize, c: usize) -> RichError { RichError { kind: k, line: l, col: c, stack: self.d_stack.clone(), f_stack: self.f_stack.clone(), word: self.c_stack.last().map(|f| self.dict[f.word_idx].name.clone()), call_stack: self.c_stack.iter().map(|f| self.dict[f.word_idx].name.clone()).collect() } }

    fn add(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(a.wrapping_add(b)); Ok(()) }
    fn sub(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(a.wrapping_sub(b)); Ok(()) }
    fn mul(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(a.wrapping_mul(b)); Ok(()) }
    fn div(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; if b == 0 { return Err(ForthErrorKind::DivisionByZero); } vm.d_stack.push(a / b); Ok(()) }
    fn dup(vm: &mut Vm) -> ForthResult<()> { let v = *vm.d_stack.last().ok_or(ForthErrorKind::StackUnderflow { exp: 1, found: 0 })?; vm.d_stack.push(v); Ok(()) }
    fn drop(vm: &mut Vm) -> ForthResult<()> { vm.pop()?; Ok(()) }
    fn swap(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(b); vm.d_stack.push(a); Ok(()) }
    fn over(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(a); vm.d_stack.push(b); vm.d_stack.push(a); Ok(()) }
    fn fadd(vm: &mut Vm) -> ForthResult<()> { let (b, a) = (vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 1, found: 0 })?, vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 2, found: 1 })?); vm.f_stack.push(a + b); Ok(()) }
    fn fsub(vm: &mut Vm) -> ForthResult<()> { let (b, a) = (vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 1, found: 0 })?, vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 2, found: 1 })?); vm.f_stack.push(a - b); Ok(()) }
    fn fmul(vm: &mut Vm) -> ForthResult<()> { let (b, a) = (vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 1, found: 0 })?, vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 2, found: 1 })?); vm.f_stack.push(a * b); Ok(()) }
    fn fdiv(vm: &mut Vm) -> ForthResult<()> { let (b, a) = (vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 1, found: 0 })?, vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 2, found: 1 })?); if b == 0.0 { return Err(ForthErrorKind::DivisionByZero); } vm.f_stack.push(a / b); Ok(()) }
    fn fdot(vm: &mut Vm) -> ForthResult<()> { print!("{} ", vm.f_stack.pop().ok_or(ForthErrorKind::StackUnderflow { exp: 1, found: 0 })?); Ok(()) }
    fn fetch(vm: &mut Vm) -> ForthResult<()> { let a = vm.pop()?; vm.d_stack.push(vm.memory.read_i64(a as usize)?); Ok(()) }
    fn store(vm: &mut Vm) -> ForthResult<()> { let a = vm.pop()?; let v = vm.pop()?; vm.memory.write_i64(a as usize, v) }
    fn dot(vm: &mut Vm) -> ForthResult<()> { print!("{} ", vm.pop()?); Ok(()) }
    fn dot_s(vm: &mut Vm) -> ForthResult<()> { println!("\x1b[32m<{}>\x1b[0m {:?}", vm.d_stack.len(), vm.d_stack); Ok(()) }
    fn emit_p(vm: &mut Vm) -> ForthResult<()> { print!("{}", vm.pop()? as u8 as char); Ok(()) }
    fn cr(_vm: &mut Vm) -> ForthResult<()> { println!(); Ok(()) }
    fn trace(vm: &mut Vm) -> ForthResult<()> { vm.tracing = !vm.tracing; println!("\x1b[1;30m(Tracing {})\x1b[0m", if vm.tracing { "ON" } else { "OFF" }); Ok(()) }
    fn bench(vm: &mut Vm) -> ForthResult<()> { vm.bench = !vm.bench; println!("\x1b[1;30m(Benchmarking {})\x1b[0m", if vm.bench { "ON" } else { "OFF" }); Ok(()) }
    fn imm_p(vm: &mut Vm) -> ForthResult<()> { if let Some(w) = vm.dict.last_mut() { w.immediate = true; } Ok(()) }
    fn bye(_vm: &mut Vm) -> ForthResult<()> { println!("Goodbye."); std::process::exit(0); }
    fn dup2(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(a); vm.d_stack.push(b); vm.d_stack.push(a); vm.d_stack.push(b); Ok(()) }
    fn drop2(vm: &mut Vm) -> ForthResult<()> { vm.pop2()?; Ok(()) }
    fn lt(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(if a < b { -1 } else { 0 }); Ok(()) }
    fn gt(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(if a > b { -1 } else { 0 }); Ok(()) }
    fn eq(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(if a == b { -1 } else { 0 }); Ok(()) }
    fn zero_eq(vm: &mut Vm) -> ForthResult<()> { let a = vm.pop()?; vm.d_stack.push(if a == 0 { -1 } else { 0 }); Ok(()) }
    fn zero_lt(vm: &mut Vm) -> ForthResult<()> { let a = vm.pop()?; vm.d_stack.push(if a < 0 { -1 } else { 0 }); Ok(()) }
    fn negate(vm: &mut Vm) -> ForthResult<()> { let a = vm.pop()?; vm.d_stack.push(a.wrapping_neg()); Ok(()) }
    fn abs_v(vm: &mut Vm) -> ForthResult<()> { let a = vm.pop()?; vm.d_stack.push(a.wrapping_abs()); Ok(()) }
    fn mod_v(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; if b == 0 { return Err(ForthErrorKind::DivisionByZero); } vm.d_stack.push(a % b); Ok(()) }
    fn max_v(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(a.max(b)); Ok(()) }
    fn min_v(vm: &mut Vm) -> ForthResult<()> { let (a, b) = vm.pop2()?; vm.d_stack.push(a.min(b)); Ok(()) }
    fn include_p(_vm: &mut Vm) -> ForthResult<()> {
        // 'include' word is handled at eval() level via the REPL/file runner.
        // If reached here it means it was used in a definition, which is unsupported.
        Err(ForthErrorKind::UnknownToken("include cannot be used inside a word definition".into()))
    }

    fn see(_vm: &mut Vm) -> ForthResult<()> { Ok(()) }

    pub fn see_word(&self, name: &str) -> ForthResult<()> {
        let idx = *self.lookup.get(name).ok_or(ForthErrorKind::WordNotFound(name.to_string()))?;
        println!("\x1b[1;36mDefinition of '{}':\x1b[0m", name);
        match self.dict[idx].kind {
            WordKind::Prim(_) => println!("  (Primitive)"),
            WordKind::Var(a) => println!("  Variable @ 0x{:X}", a),
            WordKind::Const(v) => println!("  Constant: {}", v),
            WordKind::Defined(start) => {
                let mut ip = start;
                loop {
                    let op = Op::from_u8(self.code.ops[ip])?;
                    let data = self.code.data[ip];
                    print!("  {:4} | {:10}", ip, format!("{:?}", op));
                    match op {
                        Op::Push => print!("  {}", data), Op::PushF => print!("  {}", f64::from_bits(data)),
                        Op::Prim | Op::Call => print!("  '{}'", self.dict[data as usize].name),
                        Op::Jump | Op::JZ => print!("  -> {}", data), _ => {}
                    }
                    println!(); if op == Op::Exit { break; } ip += 1;
                }
            }
        }
        Ok(())
    }
}

fn run_file(vm: &mut Vm, path: &str) -> Result<(), String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("Cannot open '{}': {}", path, e))?;
    let mut src = String::new(); f.read_to_string(&mut src).map_err(|e| e.to_string())?;
    vm.eval(&src).map_err(|e| format!("{}", e))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut vm = Vm::new();
    if args.len() >= 2 {
        println!("\x1b[1;36mkaiforth v6.0\x1b[0m — {}", &args[1]);
        if let Err(e) = run_file(&mut vm, &args[1]) { eprintln!("\x1b[31m{e}\x1b[0m"); std::process::exit(1); }
        return;
    }
    println!("\x1b[1;36mkaiforth v6.0\x1b[0m  type 'bye' to exit");
    loop {
        if vm.compiling { print!("\x1b[1;33m  ... \x1b[0m"); } else { print!("\x1b[1;32mok> \x1b[0m"); }
        if stdout().flush().is_err() { break; }
        let mut input = String::new(); if std::io::stdin().read_line(&mut input).is_err() { break; }
        if input.trim() == "bye" { break; }
        if let Err(e) = vm.eval(&input) { print!("{}", e); vm.d_stack.clear(); }
    }
}

use regex::Regex;
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

#[derive(Debug, PartialEq, Clone)]
pub enum Token { Word(String), Number(i64), Float(f64), StringLiteral(String) }

pub struct InputSource { pub text: String, pub ptr: usize }

pub struct Parser { 
    pub input_stack: Vec<InputSource>, 
    pub base: i64,
    token_re: Regex,
}

impl Parser {
    pub fn try_new() -> ForthResult<Self> {
        let re = Regex::new(r#"(?x)
            ^\s*
            (?:
                \\[^\n]*\n?
                |
                \((?:[^)]*)\)
                |
                "(?P<str>(?:[^"\\]|\\.)*)"
                |
                (?P<hex>0x[0-9a-fA-F]+)
                |
                (?P<num>-?\d+\.\d+)
                |
                (?P<int>-?\d+)
                |
                (?P<word>\S+)
            )
        "#).map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Parsing))?;
        Ok(Self { input_stack: Vec::new(), base: 10, token_re: re })
    }

    pub fn read_until(&mut self, delim: char) -> Option<String> {
        let top = self.input_stack.last_mut()?;
        let start = top.ptr;
        if let Some(pos) = top.text[start..].find(delim) {
            let s = top.text[start..start+pos].to_string();
            top.ptr = start + pos + 1;
            Some(s)
        } else {
            let s = top.text[start..].to_string();
            top.ptr = top.text.len();
            Some(s)
        }
    }

    pub fn next_token(&mut self) -> ForthResult<Option<Token>> {
        loop {
            let (is_eof, _ptr, _text_len) = if let Some(src) = self.input_stack.last() {
                (src.ptr >= src.text.len(), src.ptr, src.text.len())
            } else {
                return Ok(None);
            };

            if is_eof {
                self.input_stack.pop();
                continue;
            }

            let src = self.input_stack.last_mut().unwrap();
            
            if let Some(caps) = self.token_re.captures(&src.text[src.ptr..]) {
                let m = caps.get(0).ok_or_else(|| ForthError::new(ForthErrorKind::UnknownToken, ForthPhase::Parsing))?;
                src.ptr += m.end();
                
                if let Some(w) = caps.name("word") { return Ok(Some(Token::Word(w.as_str().to_string()))); }
                if let Some(i) = caps.name("int") { 
                    let v = i64::from_str_radix(i.as_str(), self.base as u32)
                        .map_err(|_| ForthError::new(ForthErrorKind::UnknownToken, ForthPhase::Parsing))?;
                    return Ok(Some(Token::Number(v))); 
                }
                if let Some(f) = caps.name("num") { 
                    let v = f.as_str().parse::<f64>()
                        .map_err(|_| ForthError::new(ForthErrorKind::UnknownToken, ForthPhase::Parsing))?;
                    return Ok(Some(Token::Float(v))); 
                }
                if let Some(h) = caps.name("hex") { 
                    let v = i64::from_str_radix(&h.as_str()[2..], 16)
                        .map_err(|_| ForthError::new(ForthErrorKind::UnknownToken, ForthPhase::Parsing))?;
                    return Ok(Some(Token::Number(v))); 
                }
                if let Some(s) = caps.name("str") { return Ok(Some(Token::StringLiteral(s.as_str().to_string()))); }
                continue;
            }
            src.ptr += 1;
        }
    }
}


extern crate ascii;

use std::collections::BTreeMap as Map;

#[derive(Debug, Eq, PartialEq)]
pub enum BencodeType<'a> {
    Integer(i32),
    String(&'a str),
    List(Vec<BencodeType<'a>>),
    Dictionary(Map<&'a str, BencodeType<'a>>),
}

impl<'a> BencodeType<'a> {
    fn as_int(&self) -> Option<i32> {
        match self {
            BencodeType::Integer(x) => Some(*x),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&'a str> {
        match self {
            BencodeType::String(x) => Some(x),
            _ => None,
        }
    }

    fn as_list(&self) -> Option<&[BencodeType]> {
        match self {
            BencodeType::List(ref x) => Some(x),
            _ => None,
        }
    }

    fn as_dict(&self) -> Option<&Map<&str, BencodeType>> {
        match self {
            BencodeType::Dictionary(ref x) => Some(x),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum ConvertError {
    BufferTooShort,
    InvalidFormat,
    InvalidEncoding,
    PayloadTooBig,
    EOF,
}

#[derive(Debug)]
struct ParseResult<'a> {
    value: BencodeType<'a>,
    next: &'a [u8],
}

impl<'a> ParseResult<'a> {
    fn new(value: BencodeType<'a>, next: &'a [u8]) -> ParseResult<'a> {
        ParseResult {
            value,
            next,
        }
    }

    fn done(&self) -> bool {
        self.next.is_empty()
    }
}

fn parse_int(stream: &[u8]) -> Result<ParseResult, ConvertError> {
    let i_idx = stream
        .iter()
        .position(|x| *x == b'i')
        .map_or(Err(ConvertError::InvalidFormat), Ok)?;
    if i_idx != 0 {
        return Err(ConvertError::BufferTooShort);
    }
    let e_idx = stream
        .iter()
        .position(|x| *x == b'e')
        .map_or(Err(ConvertError::InvalidFormat), Ok)?;
    if e_idx <= 1 {
        return Err(ConvertError::BufferTooShort);
    }

    let payload = &stream[1..e_idx];
    let ascii = ascii::AsciiStr::from_ascii(payload).map_err(|_| ConvertError::InvalidEncoding)?;

    let val = ascii
        .as_str()
        .parse::<i32>()
        .map_err(|_| ConvertError::PayloadTooBig)?;
    Ok(ParseResult::new(
        BencodeType::Integer(val),
        &stream[e_idx + 1..],
    ))
}

fn parse_str(stream: &[u8]) -> Result<ParseResult, ConvertError> {
    let colom_idx = stream
        .iter()
        .position(|x| *x == b':')
        .map_or(Err(ConvertError::InvalidFormat), Ok)?;

    if colom_idx == 0 || colom_idx == stream.len() - 1 {
        return Err(ConvertError::InvalidFormat);
    }

    let size_slice = &stream[..colom_idx];

    let ascii =
        ascii::AsciiStr::from_ascii(size_slice).map_err(|_| ConvertError::InvalidEncoding)?;
    let size = ascii
        .as_str()
        .parse::<i32>()
        .map_err(|_| ConvertError::PayloadTooBig)? as usize;

    let payload_slice = &stream[colom_idx + 1..colom_idx + 1 + size];

    let ascii =
        ascii::AsciiStr::from_ascii(payload_slice).map_err(|_| ConvertError::BufferTooShort)?;
    Ok(ParseResult::new(
        BencodeType::String(ascii.as_str()),
        &stream[colom_idx + 1 + size..],
    ))
}

fn parse_list(stream: &[u8]) -> Result<ParseResult, ConvertError> {
    let i_idx = stream
        .iter()
        .position(|x| *x == b'l')
        .map_or(Err(ConvertError::InvalidFormat), Ok)?;
    if i_idx != 0 {
        return Err(ConvertError::BufferTooShort);
    }

    let mut stream = &stream[1..];

    let mut res = Vec::new();

    while !stream.is_empty() && stream[0] != b'e' {
        let entry = next_rule(stream)?;
        res.push(entry.value);
        stream = entry.next;
    }

    if stream.is_empty() || stream[0] != b'e' {
        return Err(ConvertError::InvalidFormat);
    }

    Ok(ParseResult::new(BencodeType::List(res), &stream[1..]))
}

fn parse_dict(stream: &[u8]) -> Result<ParseResult, ConvertError> {
    let i_idx = stream
        .iter()
        .position(|x| *x == b'd')
        .map_or(Err(ConvertError::InvalidFormat), Ok)?;
    if i_idx != 0 {
        return Err(ConvertError::BufferTooShort);
    }

    let mut stream = &stream[1..];

    let mut res = Map::new();

    while !stream.is_empty() && stream[0] != b'e' {
        let key = next_rule(stream)?;
        let entry = next_rule(key.next)?;

        if let BencodeType::String(s) = key.value {
            res.insert(s, entry.value);
            stream = entry.next;
        } else {
            return Err(ConvertError::InvalidFormat);
        }
    }

    if stream.is_empty() || stream[0] != b'e' {
        return Err(ConvertError::InvalidFormat);
    }

    Ok(ParseResult::new(BencodeType::Dictionary(res), &stream[1..]))
}

// one lookahead function
fn next_rule(stream: &[u8]) -> Result<ParseResult, ConvertError> {
    if stream.is_empty() {
        return Err(ConvertError::EOF);
    }

    match stream[0] {
        b'0' | b'1' | b'2' | b'3' | b'4' | b'5' | b'6' | b'7' | b'8' | b'9' => parse_str(stream),
        b'i' => parse_int(stream),
        b'l' => parse_list(stream),
        b'd' => parse_dict(stream),
        _ => Err(ConvertError::InvalidFormat),
    }
}

pub fn parse(stream: &[u8]) -> Result<BencodeType, ConvertError> {
    let res = next_rule(stream)?;
    if res.done() {
        Ok(res.value)
    } else {
        Err(ConvertError::EOF)
    }
}


#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn integer() {
        parse_int(b"").expect_err("format");
        parse_int(b"ie").expect_err("format");
        parse_int(b"i12345").expect_err("format");
        parse_int(b"12345e").expect_err("format");

        let int = parse_int(b"i12345e").expect("should convert");
        println!("{}", int.value.as_int().unwrap());
        assert!(int.value.as_int().unwrap() == 12345);
        assert!(int.next.len() == 0);

        let int = parse_int(b"i-12345e").expect("should convert");
        println!("{}", int.value.as_int().unwrap());
        assert!(int.value.as_int().unwrap() == -12345);
        assert!(int.next.len() == 0);

        let int = parse_int(b"i12345esomethingelse").expect("should convert");
        println!("{}", int.value.as_int().unwrap());
        assert!(int.value.as_int().unwrap() == 12345);
        assert!(int.next.len() != 0);
        assert!(int.next[0] == b's');
    }

    #[test]
    fn str() {
        parse_str(b"").expect_err("format");

        let s = parse_str(b"3:abc").expect("format");
        println!("{}", s.value.as_str().unwrap());
        assert!(s.value.as_str().unwrap() == "abc");
        assert!(s.next.len() == 0);

        let s = parse_str(b"3:abcd").expect("format");
        println!("{}", s.value.as_str().unwrap());
        assert!(s.value.as_str().unwrap() == "abc");
        assert!(s.next.len() != 0);
        assert!(s.next[0] == b'd');
    }

    #[test]
    fn list() {
        parse_list(b"li1ei2e").expect_err("incomplete format");

        let list = parse_list(b"li1ei2ee").expect("should be an int list");
        assert!(list.value.as_list().expect("must be a list").len() == 2);
        assert!(list.value.as_list().unwrap()[0] == BencodeType::Integer(1));
        assert!(list.value.as_list().unwrap()[1] == BencodeType::Integer(2));

        let list = parse_list(b"li1ei2ee").expect("should be an int list");
        assert!(list.value.as_list().expect("must be a list").len() == 2);
        assert!(list.value.as_list().unwrap()[0] == BencodeType::Integer(1));
        assert!(list.value.as_list().unwrap()[1] == BencodeType::Integer(2));
    }

    #[test]
    fn dict() {
        parse_dict(b"di1ei2e").expect_err("int as key");
        parse_dict(b"d3:abci34e").expect_err("no ending e");

        let dict = parse_dict(b"d3:abci3ee").expect("should be a dict");
        let dict = dict.value.as_dict().expect("must be a dict");
        assert!(dict.len() == 1);
        assert!(
            dict.get("abc")
                .expect("must exist")
                .as_int()
                .expect("must be an int")
                == 3
        );
    }

    #[test]
    fn alltogether() {
        let bencode = b"d4:dictd3:key36:This is a string within a dictionarye7:integeri12345e4:listli1ei2ei3ei4e6:stringi5edee6:string11:Hello Worlde";

        let all = parse(bencode).expect("this is a correct input");
        let dict = all.as_dict().expect("top level is a dict");
        println!("{}", dict.len());
        assert!(dict.len() == 4);
    }

}

use byteorder::{LittleEndian, ReadBytesExt};
use core::panic;
use log::*;
use std::io::Cursor;
use std::process;
use poe_bundle::BundleReader;

use super::specification::FieldSpec;
use super::specification::FileSpec;
use super::util;
use super::value::Value;

const DATA_SECTION_START: &[u8; 8] = &[0xBB; 8];

pub struct DatFile {
    pub bytes: Vec<u8>,
    pub total_size: usize,
    pub rows_begin: usize,
    pub data_section: usize,
    pub rows_count: u32,
    pub row_size: usize,
}

const EMPTY_DAT: DatFile = DatFile {
    total_size: 0,
    bytes: vec![],
    rows_begin: 0,
    data_section: 0,
    rows_count: 0,
    row_size: 0,
};

impl DatFile {
    pub fn from_bytes(bytes: Vec<u8>) -> DatFile {
        if bytes.is_empty() {
            panic!("bytes was empty");
        }
        let mut c = Cursor::new(&bytes);
        let rows_count = c.read_u32::<LittleEndian>().unwrap();
        if rows_count <= 0 {
            warn!("DAT file is empty");
            return EMPTY_DAT;
        }

        let rows_begin = 4;
        let data_section = util::search_for(&bytes, DATA_SECTION_START);
        let rows_total_size = data_section - rows_begin;
        let row_size = rows_total_size / rows_count as usize;

        DatFile {
            total_size: bytes.len(),
            bytes,
            rows_begin,
            data_section,
            rows_count,
            row_size,
        }
    }
}

pub trait DatFileRead {
    fn valid(&self, spec: &FileSpec);
    fn check_offset(&self, offset: usize);
    fn read_field(&self, row: u64, field: &FieldSpec) -> Value;
    fn read_value(&self, offset: u64, field_type: &str) -> Value;
    fn read_list(&self, offset: u64, len: u64, field_type: &str) -> Vec<Value>;
}

impl DatFileRead for DatFile {

    fn valid(&self, spec: &FileSpec) {
        trace!("validate {} against specification", spec.filename);
        let last_field = spec.fields.last();
        if let Some(field) = last_field {
            let spec_row_size = field.offset + FileSpec::field_size(field);
            let diff = (self.row_size as u64).saturating_sub(spec_row_size);
            if diff != 0 {
                warn!("Rows in '{}' have {} bytes not defined in spec", spec.filename, diff);
            }
        } else {
            warn!("Spec for {} does not contain fields", spec.filename);
        }
    }

    fn check_offset(&self, offset: usize) {
        if offset > self.total_size {
            error!("Attempt to read outside DAT. This is a bug or the file is corrupted.");
            process::exit(-1);
        }
    }

    fn read_field(&self, row: u64, field: &FieldSpec) -> Value {
        trace!("reading {:?} from row {}", field, row);
        let row_offset = self.rows_begin + row as usize * self.row_size;
        let exact_offset = row_offset + field.offset as usize;
        let mut c = Cursor::new(&self.bytes[exact_offset..]);

        let mut parts = field.datatype.split("|");
        let prefix = parts.next();
        if prefix.filter(|&dtype| "list" == dtype).is_some() {
            let length = c.read_u32::<LittleEndian>().unwrap() as u64;
            let offset = c.read_u32::<LittleEndian>().unwrap() as u64;
            Value::List(self.read_list(offset, length, parts.next().unwrap()))
        } else if prefix.filter(|&dtype| "ref" == dtype).is_some() {
            let offset = c.read_u32::<LittleEndian>().unwrap();
            self.read_value(offset as u64, parts.next().unwrap())
        } else {
            c.read_value(field.datatype.as_str())
        }
    }

    fn read_value(&self, offset: u64, data_type: &str) -> Value {
        let exact_offset = self.data_section + offset as usize;
        self.check_offset(exact_offset);

        let mut c = Cursor::new(&self.bytes[exact_offset..]);
        c.read_value(data_type)
    }

    fn read_list(&self, offset: u64, len: u64, data_type: &str) -> Vec<Value> {
        let exact_offset = self.data_section + offset as usize;
        self.check_offset(exact_offset);

        let mut c = Cursor::new(&self.bytes[exact_offset..]);
        (0..len).map(|_| c.read_value(data_type)).collect()
    }
}

trait ReadBytesToValue {
    fn read_value<'a>(&mut self, tag: &str) -> Value;
    fn bool<'a>(&mut self) -> Value;
    fn u8<'a>(&mut self) -> Value;
    fn u32<'a>(&mut self) -> Value;
    fn i32<'a>(&mut self) -> Value;
    fn u64<'a>(&mut self) -> Value;
    fn utf16<'a>(&mut self) -> String;
}

impl ReadBytesToValue for Cursor<&[u8]> {

    fn read_value<'a>(&mut self, tag: &str) -> Value {
        return match tag {
            "bool" => self.bool(),
            "u8"   => self.u8(),
            "u32"  => self.u32(),
            "i32"  => self.i32(),
            "ptr"  => self.u64(),
            "u64"  => self.u64(),
            "string" => Value::Str(self.utf16()),
            value => panic!("Unsupported type in specification. {}", value),
        };
    }

    fn bool<'a>(&mut self) -> Value {
        return match self.read_u8() {
            Ok(value) => Value::Bool(value != 0),
            _ => panic!("Unable to read bool"),
        };
    }

    fn u8<'a>(&mut self) -> Value {
        return match self.read_u8() {
            Ok(value) => Value::Byte(value),
            Err(_)=> panic!("Unable to read u8"),
        };
    }

    fn u32<'a>(&mut self) -> Value {
        return match self.read_u32::<LittleEndian>() {
            Ok(value) => u32_to_enum(value),
            Err(_) => panic!("Unable to read u32"),
        };
    }

    fn i32<'a>(&mut self) -> Value {
        return match self.read_i32::<LittleEndian>() {
            Ok(value) => i32_to_enum(value),
            Err(_) => panic!("Unable to read u32"),
        };
    }

    fn u64<'a>(&mut self) -> Value {
        return match self.read_u64::<LittleEndian>() {
            Ok(value) => u64_to_enum(value),
            Err(_) => panic!("Unable to read u64"),
        };
    }

    fn utf16<'a>(&mut self) -> String {
        let raw = (0..)
            .map(|_| self.read_u16::<LittleEndian>().unwrap())
            .take_while(|&x| x != 0u16)
            .collect::<Vec<u16>>();
        return String::from_utf16(&raw).expect("Unable to decode as UTF-16 String");
    }
}

fn u64_to_enum(value: u64) -> Value {
    if value == 0xFEFEFEFEFEFEFEFE {
        return Value::Empty;
    }
    return Value::U64(value);
}

fn u32_to_enum(value: u32) -> Value {
    if value == 0xFEFEFEFE {
        return Value::Empty;
    }
    return Value::U64(value as u64);
}

fn i32_to_enum(value: i32) -> Value {
    // TODO: check for empty signal
    return Value::I64(value as i64);
}
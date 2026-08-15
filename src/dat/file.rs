use byteorder::{LittleEndian, ReadBytesExt};
use std::fmt::Error;
use log::*;
use std::io::Cursor;
use crate::error::QueryError;
use crate::traversal::value::Value;
use crate::traversal::value::Value::U64;

use super::specification::FieldSpec;
use super::specification::FileSpec;
use super::util;

const DATA_SECTION_MARKER: &[u8; 8] = &[0xBB; 8];

pub struct DatFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub total_size: usize,
    pub rows_begin: usize,
    pub data_section: usize,
    pub rows_count: u32,
    pub row_size: usize,
}

impl std::fmt::Debug for DatFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} {} rows ({} bytes)", self.name, self.rows_count, self.total_size)
    }
}

impl DatFile {

    pub fn from_bytes(name: String, bytes: Vec<u8>) -> Result<DatFile, QueryError> {
        if bytes.is_empty() {
            return Err(QueryError::internal(format!("{}: no data provided to read the file from", name)));
        }
        let mut cursor = Cursor::new(&bytes);
        let Ok(rows_count) = cursor.read_u32::<LittleEndian>() else {
            return Err(QueryError::internal(format!("{}: DAT file is empty", name)));
        };

        let rows_begin = 4;
        let Some(data_section) = util::search_for(&bytes, DATA_SECTION_MARKER) else {
            return Err(QueryError::internal(format!("{}: data section marker not found, file is corrupt", name)));
        };
        let rows_total_size = data_section - rows_begin;
        let row_size = match rows_count {
            0 => 0,
            rows => rows_total_size / rows as usize,
        };

        let file = DatFile {
            name,
            total_size: bytes.len(),
            bytes,
            rows_begin,
            data_section,
            rows_count,
            row_size
        };

        info!("Read {:?}", file);
        Ok(file)
    }

    pub fn valid(&self, spec: &FileSpec) {
        debug!("Validating using specification '{}'", spec);
        let last_field = spec.file_fields.last();
        if let Some(field) = last_field {
            let spec_row_size = field.field_offset + FileSpec::field_size(field);
            if self.row_size > spec_row_size {
                warn!("Spec for '{}' missing {} bytes", spec.file_name, self.row_size - spec_row_size);
            }
            if spec_row_size > self.row_size {
                warn!("Spec for '{}' overflows by {} bytes", spec.file_name, spec_row_size - self.row_size);
            }
        } else {
            warn!("Spec for {} does not contain fields", spec.file_name);
        }
    }

    pub fn check_offset(&self, offset: usize) -> Result<(), QueryError> {
        if offset > self.total_size {
            return Err(QueryError::internal(format!(
                "attempt to read outside the file {} (offset {}, size {}). \
                This is most likely an incorrect specification or a corrupt DAT file. \
                It can be reported here: https://github.com/ex-nihil/poe-query/issues",
                self.name, offset, self.total_size)));
        }
        Ok(())
    }

    pub fn read_field(&self, row: u64, field: &FieldSpec) -> Result<Value, QueryError> {
        let row_offset = self.rows_begin + row as usize * self.row_size;
        let exact_offset = row_offset + field.field_offset;

        if field.field_offset > self.row_size {
            // Spec describes more data than is in the row
            return Ok(Value::Empty);
        }
        self.check_offset(exact_offset)?;

        let mut cursor = Cursor::new(&self.bytes[exact_offset..]);

        let mut parts = field.field_type.split('|');
        let prefix = parts.next();
        let result = if let Some(enum_spec) = &field.enum_name {
            match cursor.u32()? {
                Value::U64(v) => Value::Str(enum_spec.value(v as usize)),
                Value::Empty => Value::Empty,
                x => return Err(QueryError::internal(format!("reading {} from row {} - got {:?}", field, row, x)))
            }
        } else if prefix.filter(|&dtype| "list" == dtype).is_some() {
            let length = cursor.u64()?;
            let offset = cursor.u64()?;
            match (offset, length) {
                (Value::U64(o), Value::U64(len)) => {
                    let item_type = parts.next()
                        .ok_or_else(|| QueryError::internal(format!("list field {} is missing its item type", field)))?;
                    Value::List(self.read_list(o, len, item_type)?)
                }
                _ => Value::Empty
            }
        } else if prefix.filter(|&dtype| "ref" == dtype).is_some() {
            match cursor.u64()? {
                Value::U64(offset) => {
                    let item_type = parts.next()
                        .ok_or_else(|| QueryError::internal(format!("ref field {} is missing its item type", field)))?;
                    self.read_value(offset, item_type)?
                }
                Value::Empty => Value::Empty,
                x => return Err(QueryError::internal(format!("reading {} from row {} - got {:?}", field, row, x)))
            }
        } else {
            cursor.read_value(field.field_type.as_str())?
        };
        debug!("Result {}[{}] = {:?}", field, row, result);
        Ok(result)
    }

    pub fn read_value(&self, offset: u64, data_type: &str) -> Result<Value, QueryError> {
        let exact_offset = self.data_section + offset as usize;
        self.check_offset(exact_offset)?;

        let mut cursor = Cursor::new(&self.bytes[exact_offset..]);
        cursor.read_value(data_type)
    }

    pub fn read_list(&self, offset: u64, len: u64, data_type: &str) -> Result<Vec<Value>, QueryError> {
        let exact_offset = self.data_section + offset as usize;
        self.check_offset(exact_offset)?;

        let mut cursor = Cursor::new(&self.bytes[exact_offset..]);
        (0..len).map(|_| {
            match data_type {
                "string" | "path" => {
                    let U64(offset) = cursor.u64()? else {
                        return Err(QueryError::internal(format!("{}: unable to read offset to string list element", self.name)));
                    };
                    let element_offset = self.data_section + offset as usize;
                    self.check_offset(element_offset)?;
                    let mut text_cursor = Cursor::new(&self.bytes[element_offset..]);
                    text_cursor.read_value(data_type)
                },
                _ => cursor.read_value(data_type)
            }
        }).collect()
    }
}

trait ReadBytesToValue {
    fn read_value(&mut self, tag: &str) -> Result<Value, QueryError>;
    fn bool(&mut self) -> Result<Value, QueryError>;
    fn u8(&mut self) -> Result<Value, QueryError>;
    fn u16(&mut self) -> Result<Value, QueryError>;
    fn i16(&mut self) -> Result<Value, QueryError>;
    fn u32(&mut self) -> Result<Value, QueryError>;
    fn i32(&mut self) -> Result<Value, QueryError>;
    fn f32(&mut self) -> Result<Value, QueryError>;
    fn u64(&mut self) -> Result<Value, QueryError>;
    fn utf16(&mut self) -> Result<String, QueryError>;
    fn utf8(&mut self) -> Result<String, QueryError>;
}

fn truncated(data_type: &str) -> QueryError {
    QueryError::internal(format!("unable to read {}, unexpected end of data", data_type))
}

impl ReadBytesToValue for Cursor<&[u8]> {

    fn read_value(&mut self, tag: &str) -> Result<Value, QueryError> {
        match tag {
            "bool" => self.bool(),
            "u8"   => self.u8(),
            "u16"  => self.u16(),
            "i16"  => self.i16(),
            "u32"  => self.u32(),
            "i32"  => self.i32(),
            "f32"  => self.f32(),
            "ptr"  => self.u64(),
            "u64"  => self.u64(),
            "string" => Ok(Value::Str(self.utf16()?)),
            "path" => Ok(Value::Str(self.utf8()?)),
            "_" => Ok(Value::Empty),
            value => Err(QueryError::internal(format!("unsupported type '{}' in specification", value))),
        }
    }

    // I've seen booleans return both 1 and 254, what's the significance?
    fn bool(&mut self) -> Result<Value, QueryError> {
        match self.read_u8() {
            Ok(0) => Ok(Value::Bool(false)),
            Ok(1) => Ok(Value::Bool(true)),
            Ok(255) => Ok(Value::Bool(true)),
            Ok(value) => {
                warn!("Expected boolean value got {}", value);
                Ok(Value::Bool(true))
            },
            _ => Err(truncated("bool")),
        }
    }

    fn u8(&mut self) -> Result<Value, QueryError> {
        self.read_u8()
            .map(Value::Byte)
            .map_err(|_| truncated("u8"))
    }

    fn u16(&mut self) -> Result<Value, QueryError> {
        self.read_u16::<LittleEndian>()
            .map(|value| Value::U64(value as u64))
            .map_err(|_| truncated("u16"))
    }

    fn i16(&mut self) -> Result<Value, QueryError> {
        self.read_i16::<LittleEndian>()
            .map(|value| Value::I64(value as i64))
            .map_err(|_| truncated("i16"))
    }

    fn u32(&mut self) -> Result<Value, QueryError> {
        self.read_u32::<LittleEndian>()
            .map(u32_to_enum)
            .map_err(|_| truncated("u32"))
    }

    fn i32(&mut self) -> Result<Value, QueryError> {
        self.read_i32::<LittleEndian>()
            .map(i32_to_enum)
            .map_err(|_| truncated("i32"))
    }

    fn f32(&mut self) -> Result<Value, QueryError> {
        self.read_f32::<LittleEndian>()
            .map(f32_to_enum)
            .map_err(|_| truncated("f32"))
    }

    fn u64(&mut self) -> Result<Value, QueryError> {
        self.read_u64::<LittleEndian>()
            .map(u64_to_enum)
            .map_err(|_| truncated("u64"))
    }

    fn utf16(&mut self) -> Result<String, QueryError> {
        let mut raw = Vec::new();
        loop {
            match self.read_u16::<LittleEndian>() {
                Ok(0) => break,
                Ok(value) => raw.push(value),
                Err(_) => return Err(truncated("string")),
            }
        }
        String::from_utf16(&raw)
            .map_err(|_| QueryError::internal("unable to decode as UTF-16 String"))
    }

    fn utf8(&mut self) -> Result<String, QueryError> {
        let mut raw = Vec::new();
        loop {
            match self.read_u16::<LittleEndian>() {
                Ok(0) => break,
                Ok(value) => raw.push(value as u8),
                Err(_) => return Err(truncated("path")),
            }
        }
        String::from_utf8(raw)
            .map_err(|_| QueryError::internal("unable to decode as UTF-8 String"))
    }
}

fn u64_to_enum(value: u64) -> Value {
    if value == 0xFEFEFEFEFEFEFEFE {
        return Value::Empty;
    }
    Value::U64(value)
}

fn u32_to_enum(value: u32) -> Value {
    if value == 0xFEFEFEFE {
        return Value::Empty;
    }
    Value::U64(value as u64)
}

fn i32_to_enum(value: i32) -> Value {
    // TODO: check for empty signal
    Value::I64(value as i64)
}

fn f32_to_enum(value: f32) -> Value {
    Value::F32(value)
}

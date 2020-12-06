use std::collections::HashMap;
use std::fs;

use poe_bundle_reader::reader::{BundleReader, BundleReaderRead};

use super::dat_file::DatFile;
use super::dat_file::DatValue;
use super::dat_navigate::DatNavigate;
use super::dat_spec::FileSpec;

pub struct DatContainer {
    files: HashMap<String, DatFile>,
    specs: HashMap<String, FileSpec>,
}

// TODO: lazy loading
impl DatContainer {
    pub fn from_install(path: &str, spec_path: &str) -> DatContainer {
        let paths = fs::read_dir(spec_path).expect("spec path does not exist");
        let specs: HashMap<String, FileSpec> = paths
            .filter_map(Result::ok)
            .map(|d| d.path())
            .filter(|pb| pb.is_file() && pb.extension().unwrap().to_string_lossy() == "yaml")
            .map(|pb| {
                let spec = FileSpec::read(pb.as_path());
                (spec.filename.clone(), spec)
            })
            .collect();

        let bundles = BundleReader::from_install(path);

        let dat_files: HashMap<String, DatFile> = specs
            .keys()
            .map(|path| {
                let size = bundles.size_of(path).unwrap();
                let mut dst = Vec::with_capacity(size);
                bundles
                    .write_into(path, &mut dst)
                    .expect("failed writing DAT file to buffer");
                (path.clone(), DatFile::from_bytes(dst))
            })
            .collect();

        DatContainer {
            files: dat_files,
            specs,
        }
    }
}

pub trait DatContainerImpl {
    fn navigate(&self) -> DatNavigate;
}

impl DatContainerImpl for DatContainer {
    fn navigate(&self) -> DatNavigate {
        let exports: Vec<DatValue> = self
            .specs
            .values()
            .map(|spec| {
                let file = self.files.get(&spec.filename).unwrap();
                
                DatValue::KeyValue(
                    spec.export.to_string(),
                    Box::new(DatValue::List(vec![DatValue::Str(format!("list containing {} rows", file.rows_count))])),
                )
            })
            .collect();

        DatNavigate {
            files: &self.files,
            specs: &self.specs,
            current_field: None,
            current_file: None,
            current_value: Some(DatValue::Object(Box::new(DatValue::List(exports)))),
        }
    }
}

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use log::error;

use poe_bundle::reader::{BundleReader, BundleReaderRead};
use crate::dat::specification::EnumSpec;

use super::file::DatFile;
use super::file::DatFileRead;
use super::specification::FileSpec;
use super::traverse::TraversalContext;

pub struct DatContainer<'a> {
    files: HashMap<String, DatFile>,
    specs: HashMap<String, FileSpec>,
    enums: HashMap<String, EnumSpec>,
    _marker: PhantomData<&'a ()>
}

// TODO: lazy loading of dat files
impl<'a> DatContainer<'a> {

    pub fn from_install(bundles: &BundleReader, spec_path: &str) -> DatContainer<'a> {
        let enums = FileSpec::read_all_enum_specs(spec_path);
        let specs = FileSpec::read_all_specs(spec_path, &enums);

        let mut dat_files: HashMap<String, DatFile> = HashMap::new();

        for (path, spec) in &specs {
            if None == bundles.size_of(path) {
                error!("Unable to read {}", path);
                continue;
            }

            let file = DatFile::from_bytes(bundles.bytes(path).unwrap());
            file.valid(spec);
            //(path.clone(), file)
            dat_files.insert(path.clone(), file);
        }

        DatContainer {
            files: dat_files,
            specs,
            enums,
            _marker: Default::default()
        }
    }
}

pub trait DatStoreImpl<'a> {
    fn file(&self, path: &str) -> Option<&DatFile>;
    fn spec(&self, path: &str) -> Option<&FileSpec>;
    fn spec_by_export(&self, export: &str) -> Option<&FileSpec>;
    fn exports(&self) -> HashSet<&str>;
    fn enum_name(&self, path: &str) -> Option<&EnumSpec>;
}

impl<'a> DatStoreImpl<'a> for DatContainer<'a> {
    fn file(&self, path: &str) -> Option<&DatFile> {
        self.files.get(path)
    }

    fn spec(&self, path: &str) -> Option<&FileSpec> {
        self.specs.get(path)
    }

    fn spec_by_export(&self, export: &str) -> Option<&FileSpec> {
        self.specs.values().find(|s| s.export == export)
    }

    fn exports(&self) -> HashSet<&str> {
        self.specs.iter().map(|(_, s)| s.export.as_str()).collect()
    }

    fn enum_name(&self, path: &str) -> Option<&EnumSpec> {
        self.enums.get(path)
    }
}

pub trait DatContainerImpl {
    fn navigate(&self) -> TraversalContext;
}

impl DatContainerImpl for DatContainer<'_> {
    fn navigate(&self) -> TraversalContext {
        TraversalContext {
            //cont: self,
            store: self,
            variables: HashMap::new(),
            current_field: None,
            current_file: None,
            identity: None,
        }
    }
}
//! Catalogs: the kinds of things the game is made of, one file each in a
//! folder of `assets` (`effects`, `weapons`, `moves`, `actors`), read with
//! the generic `data::read`. `build.rs` makes each folder an id enum, a
//! variant per file (`assets/weapons/sweep.ron` is `WeaponId::Sweep`), and
//! bundles the files' texts, so code and data name things by id and never
//! by a string: a new thing is a new file, and a new kind is a new folder,
//! a line in `build.rs`'s list and the type its files read into.

use std::marker::PhantomData;

use bevy::prelude::*;
use serde::de::DeserializeOwned;

use crate::content::data;
use crate::model::*;

/// A catalog id whose files are bundled: each one's text, by index.
pub trait Bundled: CatalogId {
    const TEXTS: &'static [&'static str];
}

include!(concat!(env!("OUT_DIR"), "/catalog_texts.rs"));

/// Every thing of one kind, read into `Spec`, by its id. The bundled files
/// by default; another (a test's, a mod's) can be read from other text.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Catalog<Id: CatalogId, Spec: Send + Sync + 'static> {
    specs: Vec<Option<Spec>>,
    ids: PhantomData<fn() -> Id>,
}

impl<Id: CatalogId, Spec: Send + Sync + 'static> Default for Catalog<Id, Spec> {
    fn default() -> Self {
        Self {
            specs: Id::ALL.iter().map(|_| None).collect(),
            ids: PhantomData,
        }
    }
}

impl<Id: CatalogId, Spec: Send + Sync + 'static> Catalog<Id, Spec> {
    /// The things in `(id, text)` files.
    pub fn read<'a>(files: impl IntoIterator<Item = (Id, &'a str)>) -> Result<Self, String>
    where
        Spec: DeserializeOwned,
    {
        let mut catalog = Self::default();
        for (id, text) in files {
            catalog.specs[id.index()] = Some(data::read(id.file(), text)?);
        }
        Ok(catalog)
    }

    /// The thing `id`, if this catalog has it.
    pub fn get(&self, id: Id) -> Option<&Spec> {
        self.specs.get(id.index()).and_then(Option::as_ref)
    }

    /// Every thing it has, with its id.
    pub fn iter(&self) -> impl Iterator<Item = (Id, &Spec)> {
        Id::ALL
            .iter()
            .filter_map(|&id| self.get(id).map(|spec| (id, spec)))
    }
}

impl<Id: Bundled, Spec: DeserializeOwned + Send + Sync + 'static> Catalog<Id, Spec> {
    /// Every bundled file of the kind.
    pub fn bundled() -> Result<Self, String> {
        Self::read(Id::ALL.iter().map(|&id| (id, Id::TEXTS[id.index()])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_file_is_an_id_with_its_text() {
        assert_eq!(EffectId::ALL.len(), EffectId::TEXTS.len());
        assert_eq!(EffectId::Blood.file(), "blood");
        assert_eq!(WeaponId::Sweep.file(), "sweep");
        assert!(ActorId::ALL
            .iter()
            .all(|id| !ActorId::TEXTS[id.index()].is_empty()));
    }
}

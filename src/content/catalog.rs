//! Catalogs: the kinds of things the game is made of, one file each in a
//! folder of `assets`, read with the generic `data::read`. `build.rs`'s
//! `CATALOGS` list makes each folder an id enum, a variant per file
//! (`assets/weapons/sweep.ron` is `WeaponId::Sweep`), names the spec type
//! its files read into and bundles their texts, so code and data name
//! things by id and never by a string: a new thing is a new file, and a new
//! kind is a new folder, its spec type and a line in that list.

use std::marker::PhantomData;

use bevy::prelude::*;
use serde::de::DeserializeOwned;

use crate::content::data;
use crate::model::*;

/// A catalog id whose files are bundled: the type they read into, and each
/// one's text, by index.
pub trait Bundled: CatalogId {
    type Spec: DeserializeOwned + Send + Sync + 'static;
    const TEXTS: &'static [&'static str];
}

include!(concat!(env!("OUT_DIR"), "/catalog_texts.rs"));

/// Every thing of one kind, read into its spec, by its id. The bundled
/// files by default (`bundled`); another (a test's, a mod's) can be read
/// from other text (`read`).
#[derive(Resource)]
pub struct Catalog<Id: Bundled> {
    specs: Vec<Option<Id::Spec>>,
    ids: PhantomData<fn() -> Id>,
}

impl<Id: Bundled> Default for Catalog<Id> {
    fn default() -> Self {
        Self {
            specs: Id::ALL.iter().map(|_| None).collect(),
            ids: PhantomData,
        }
    }
}

impl<Id: Bundled> Catalog<Id> {
    /// Every bundled file of the kind.
    pub fn bundled() -> Result<Self, String> {
        Self::read(Id::ALL.iter().map(|&id| (id, Id::TEXTS[id.index()])))
    }

    /// The things in `(id, text)` files: one not given is missing, and a
    /// later text for an id replaces an earlier one.
    pub fn read<'a>(files: impl IntoIterator<Item = (Id, &'a str)>) -> Result<Self, String> {
        let mut catalog = Self::default();
        for (id, text) in files {
            catalog.specs[id.index()] = Some(data::read(&id.path(), text)?);
        }
        Ok(catalog)
    }

    /// The thing `id`, if this catalog has it.
    pub fn get(&self, id: Id) -> Option<&Id::Spec> {
        self.specs.get(id.index()).and_then(Option::as_ref)
    }

    /// Every thing it has, with its id, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (Id, &Id::Spec)> {
        Id::ALL
            .iter()
            .filter_map(|&id| self.get(id).map(|spec| (id, spec)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_file_is_an_id_with_its_text() {
        assert_eq!(EffectId::ALL.len(), EffectId::TEXTS.len());
        assert_eq!(EffectId::Blood.file(), "blood");
        assert_eq!(WeaponId::Sweep.path(), "assets/weapons/sweep.ron");
        assert!(ActorId::ALL
            .iter()
            .all(|id| !ActorId::TEXTS[id.index()].is_empty()));
    }
}

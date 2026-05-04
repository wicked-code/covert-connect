use std::{
    num::NonZeroU32,
    ops::{Deref, Index, IndexMut},
};
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ItemId(NonZeroU32);

pub struct ItemPool<T> {
    items: Vec<T>,
}

impl<T> ItemPool<T> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn insert(&mut self, item: T) -> ItemId {
        self.items.push(item);
        ItemId(NonZeroU32::new(self.items.len() as u32).unwrap())
    }

    pub fn get(&self, id: ItemId) -> Option<&T> {
        self.items.get((id.get() as usize) - 1)
    }

    pub fn get_mut(&mut self, id: ItemId) -> Option<&mut T> {
        self.items.get_mut((id.get() as usize) - 1)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl<T> Index<ItemId> for ItemPool<T> {
    type Output = T;

    fn index(&self, index: ItemId) -> &Self::Output {
        self.get(index).expect("Index out of bounds")
    }
}

impl<T> IndexMut<ItemId> for ItemPool<T> {
    fn index_mut(&mut self, index: ItemId) -> &mut Self::Output {
        self.get_mut(index).expect("Index out of bounds")
    }
}

impl Deref for ItemId {
    type Target = NonZeroU32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<NonZeroU32> for ItemId {
    fn from(v: NonZeroU32) -> ItemId {
        ItemId(v)
    }
}

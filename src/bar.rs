//! The bar: the renderer and the item snapshots that it shows.

use std::collections::HashMap;

use crate::config::Config;
use crate::error::BarrsError;
use crate::ipc::EventPayload;
use crate::render::{RenderItemSnapshot, Renderer};

/// The renderer and the snapshot of each item that it shows now.
///
/// A snapshot becomes a shown item only when the renderer draws it, so
/// `dump-state` reads the same items that the renderer has.
pub struct Bar<R: Renderer> {
    renderer: R,
    items: HashMap<String, RenderItemSnapshot>,
}

impl<R: Renderer> Bar<R> {
    /// Creates a bar that shows no items yet.
    pub fn new(renderer: R) -> Self {
        Self {
            renderer,
            items: HashMap::new(),
        }
    }

    /// Replaces every shown item with `snapshots`, the complete set of
    /// `config`.
    ///
    /// # Errors
    ///
    /// Returns the renderer error; the shown items do not change.
    pub fn reconcile(
        &mut self,
        config: &Config,
        snapshots: Vec<RenderItemSnapshot>,
    ) -> Result<(), BarrsError> {
        self.renderer.reconcile(config, &snapshots)?;
        self.items = snapshots
            .into_iter()
            .map(|snapshot| (snapshot.id.clone(), snapshot))
            .collect();
        Ok(())
    }

    /// Shows a new snapshot of one item.
    ///
    /// # Errors
    ///
    /// Returns the renderer error; the shown item does not change.
    pub fn show(&mut self, snapshot: RenderItemSnapshot) -> Result<(), BarrsError> {
        self.renderer.render_item(&snapshot)?;
        if let Some(shown) = self.items.get_mut(&snapshot.id) {
            *shown = snapshot;
        } else {
            self.items.insert(snapshot.id.clone(), snapshot);
        }
        Ok(())
    }

    /// Returns the input events that the renderer collected.
    ///
    /// # Errors
    ///
    /// Returns an error when the renderer cannot read its event queue.
    pub fn drain_events(&mut self) -> Result<Vec<EventPayload>, BarrsError> {
        self.renderer.drain_events()
    }

    /// Applies an item event, such as a hover, to the renderer.
    ///
    /// # Errors
    ///
    /// Returns an error when the renderer cannot apply the event.
    pub fn handle_event(&mut self, event: &EventPayload) -> Result<(), BarrsError> {
        self.renderer.handle_event(event)
    }

    /// Returns the shown snapshot of each item, by item id.
    pub const fn items(&self) -> &HashMap<String, RenderItemSnapshot> {
        &self.items
    }

    #[cfg(test)]
    pub const fn renderer(&self) -> &R {
        &self.renderer
    }
}

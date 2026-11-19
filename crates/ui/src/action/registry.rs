// SPDX-License-Identifier: GPL-3.0-or-later

//! The registry of actions (ADR-0048 AM1–AM4, AM6–AM8): one table for the whole program, with
//! the contexts, each action's shortcuts (its defaults for the platform, or the user's), its
//! state, and the lookup from a shortcut to an action.
//!
//! # Layout in memory
//! - The actions are a `Vec` indexed by [`ActionId`]: an action's index is given at registration
//!   and never changes, so every question about an action is an index, never a string compared
//!   or hashed (AM1).
//! - The lookup is a hash map from (context, shortcut) to [`ActionId`] (AM4), kept up to date at
//!   every change of shortcuts (a handful of entries per change). A key costs one probe per
//!   candidate and per active context: usually two or three, each O(1), none allocating.
//! - The contexts are a short `Vec` of names; a [`Context`] is its index (0 is *Global*).
//!
//! # Rules kept by every change (AM7, AM8)
//! - No action has a fixed key ([`Shortcut::is_fixed`]).
//! - No two actions of the **same context** share a shortcut (a conflict). A context's shortcut
//!   that covers a global one is allowed (AM3): the lookup tries the context first.
//! - The defaults are checked at registration, so a program whose own tables conflict fails at
//!   start-up, in its tests, and never in a user's hands.
//!
//! # The user's shortcuts (AM7)
//! [`Registry::set_shortcuts`] replaces an action's list (refusing fixed keys and conflicts: the
//! shortcut editor of F2.12c resolves them with the user first); [`Registry::restore`] and
//! [`Registry::restore_all`] bring the defaults back. [`Registry::overrides`] lists only the
//! differences from the defaults, which is what the settings file stores (F2.12), and
//! [`Registry::load_overrides`] applies them as the file has them: the **later entry wins** a
//! conflict, an unknown action is kept (to be written back) and ignored, and every problem is
//! reported as a [`LoadWarning`], never as an error that would stop the program.
//!
//! # The texts people read (F2.11d2a, choice S1)
//! Once the application hands in the names ([`Registry::set_texts`]: the actions' names from its
//! catalogues and the keys' names, [`KeyNames`]), the registry keeps, next to each action, its
//! name and the text of its **first** shortcut (the one the menus show, AM6), written in the
//! platform's style ([`Shortcut::write_label`]). They are written again only when the language or
//! that action's shortcuts change, so a menu that opens copies them and formats nothing.
//!
//! # What changed (AM2, choice S3 of F2.11d2a)
//! Every change of an action's state, shortcuts or texts sets the action's bit in a bit set sized
//! at registration (one `u64` per 64 actions, so marking never allocates). The interface drains
//! it at the end of every frame ([`Registry::drain_changed`]) and records again the items of the
//! open menus whose action changed; `muda` on macOS will read the same marks (F2.11e).

use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use super::label::KeyNames;
use super::shortcut::{Platform, Shortcut};
use crate::draw::glyph_cache::KeyHasher;

/// The lookup's map. Hashed with the glyph cache's FxHash ([`KeyHasher`]) rather than SipHash:
/// the keys are small values the interface builds from key presses, nothing an adversary can
/// choose to collide, and the lookup is on the path of every unhandled key (AM15's budget).
type Lookup = HashMap<(Context, Shortcut), ActionId, BuildHasherDefault<KeyHasher>>;

/// The index of an action in the registry (AM1): given at registration, stable while the program
/// runs. The settings file never stores it (it stores the action's text identifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(u16);

impl ActionId {
    /// The index as a `usize`, for the registry's table.
    const fn index(self) -> usize {
        self.0 as usize
    }

    /// The index, as a number (for messages and tests).
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A context of shortcuts (AM3): *Global*, or the context of a view. A view's root widget declares
/// its context ([`crate::Widget::context`]); a key is looked up in the contexts from the focused
/// widget up to the root, innermost first, and *Global* last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Context(u8);

impl Context {
    /// The context that is always active, tried last.
    pub const GLOBAL: Self = Self(0);
}

/// What a standard editing action does on the focused widget (AM13): Cut, Copy, Paste, Select
/// all, Undo and Redo ask the focused widget first ([`crate::Widget::edit_state`]); a text field
/// answers them on its text and its local undo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditRole {
    /// Cut the selection.
    Cut,
    /// Copy the selection.
    Copy,
    /// Paste.
    Paste,
    /// Select everything.
    SelectAll,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
}

/// The default shortcuts of an action on each platform (AM6), possibly none, possibly several
/// (Redo on Windows has two). The first of a list is the one the menus show (F2.11d2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Defaults {
    /// On Linux.
    pub linux: Vec<Shortcut>,
    /// On Windows.
    pub windows: Vec<Shortcut>,
    /// On macOS.
    pub macos: Vec<Shortcut>,
}

impl Defaults {
    /// The same shortcuts on the three platforms (they are written with the abstract modifiers,
    /// so Ctrl+Z on Linux and Windows is ⌘Z on macOS).
    #[must_use]
    pub fn everywhere(shortcuts: &[Shortcut]) -> Self {
        Self {
            linux: shortcuts.to_vec(),
            windows: shortcuts.to_vec(),
            macos: shortcuts.to_vec(),
        }
    }

    /// The list of `platform`.
    #[must_use]
    pub fn of(&self, platform: Platform) -> &[Shortcut] {
        match platform {
            Platform::Linux => &self.linux,
            Platform::Windows => &self.windows,
            Platform::MacOs => &self.macos,
        }
    }
}

/// An action as the application describes it to the registry (AM1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    /// The stable identifier, stored by the settings file: lower case ASCII, a group and a name
    /// separated by one dot (`edit.undo`, `view.full-screen`); letters, digits and hyphens.
    pub identifier: &'static str,
    /// The Fluent key of its name (CLAUDE.md §7); the application formats it.
    pub name: &'static str,
    /// The context its shortcuts belong to.
    pub context: Context,
    /// Its default shortcuts on each platform.
    pub defaults: Defaults,
    /// `true` when holding the key repeats the action (zoom, nudges); otherwise a repeat is
    /// consumed and does nothing (Save runs once per press, AM4).
    pub repeatable: bool,
    /// The standard editing action it is, if any (AM13).
    pub edit: Option<EditRole>,
}

/// An action's state (AM2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionState {
    /// `false` while the action can do nothing: its shortcut is consumed and nothing runs, its
    /// menu item is shown disabled (F2.11d2).
    pub enabled: bool,
    /// For a toggle or a choice, whether it is checked; `None` for an ordinary action.
    pub checked: Option<bool>,
}

impl ActionState {
    /// Enabled, not a toggle: the state of a new action.
    pub const ENABLED: Self = Self {
        enabled: true,
        checked: None,
    };

    /// Disabled, not a toggle.
    pub const DISABLED: Self = Self {
        enabled: false,
        checked: None,
    };
}

/// Why the registry refused a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// The identifier is not `group.name` in lower case ASCII letters, digits and hyphens.
    MalformedIdentifier(&'static str),
    /// Another action already has this identifier.
    DuplicateIdentifier(&'static str),
    /// The context was not created by this registry.
    UnknownContext(Context),
    /// A context with this name exists already.
    DuplicateContext(&'static str),
    /// More contexts than a [`Context`] can count (255 besides *Global*).
    TooManyContexts,
    /// More actions than an [`ActionId`] can count (65 536).
    TooManyActions,
    /// The action does not exist in this registry.
    UnknownAction(ActionId),
    /// The shortcut is a fixed key (AM8) on this platform.
    FixedKey(Shortcut),
    /// The shortcut is taken by another action of the same context (AM7).
    Conflict {
        /// The shortcut.
        shortcut: Shortcut,
        /// The action that has it.
        owner: ActionId,
    },
}

impl std::fmt::Display for RegistryError {
    /// One English line for developers; the shortcut editor (F2.12c) shows its own texts.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedIdentifier(identifier) => {
                write!(f, "{identifier:?} is not an action identifier (group.name)")
            }
            Self::DuplicateIdentifier(identifier) => {
                write!(f, "the action {identifier:?} is registered twice")
            }
            Self::UnknownContext(context) => write!(f, "no context has the index {}", context.0),
            Self::DuplicateContext(name) => write!(f, "the context {name:?} is created twice"),
            Self::TooManyContexts => f.write_str("too many contexts"),
            Self::TooManyActions => f.write_str("too many actions"),
            Self::UnknownAction(id) => write!(f, "no action has the index {}", id.0),
            Self::FixedKey(shortcut) => write!(f, "{shortcut:?} is a fixed key"),
            Self::Conflict { shortcut, owner } => {
                write!(f, "{shortcut:?} belongs to the action {} already", owner.0)
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// A problem met while loading the user's shortcuts (AM7): reported, never fatal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWarning {
    /// No action has this identifier (an action removed): the entry is kept and ignored.
    UnknownAction(String),
    /// A shortcut is a fixed key on this platform: it was dropped from the action's list.
    FixedKey {
        /// The action.
        action: ActionId,
        /// The shortcut dropped.
        shortcut: Shortcut,
    },
    /// Two actions had the same shortcut in the same context: the later entry kept it.
    Conflict {
        /// The shortcut.
        shortcut: Shortcut,
        /// The action that kept it (the later entry).
        winner: ActionId,
        /// The action that lost it.
        loser: ActionId,
    },
}

/// One registered action.
#[derive(Debug, Clone)]
struct Entry {
    /// How the application described it.
    spec: ActionSpec,
    /// Its default shortcuts on this registry's platform.
    defaults: Vec<Shortcut>,
    /// Its shortcuts now: the defaults, or the user's.
    shortcuts: Vec<Shortcut>,
    /// Its state.
    state: ActionState,
    /// Its name as people read it, from the catalogue ([`Registry::set_texts`]); empty until the
    /// texts are given.
    name_text: String,
    /// Its first shortcut written for people ([`Shortcut::write_label`]); empty without a
    /// shortcut or until the texts are given.
    shortcut_text: String,
    /// Its menu item's mnemonic, in lower case ([`Registry::set_mnemonics`], F2.11d2b).
    mnemonic: Option<char>,
}

/// The registry of actions (see the module documentation).
#[derive(Debug, Clone)]
pub struct Registry {
    /// The platform whose defaults and fixed keys apply.
    platform: Platform,
    /// The contexts' names; the index is the [`Context`]. The first is *Global*.
    contexts: Vec<&'static str>,
    /// The actions, by [`ActionId`].
    actions: Vec<Entry>,
    /// The lookup: (context, shortcut) → action.
    lookup: Lookup,
    /// The user's entries for actions that do not exist (AM7): kept, to be written back.
    unknown: Vec<(String, Vec<Shortcut>)>,
    /// The keys' names in the interface's language, once given ([`Registry::set_texts`]): the
    /// shortcuts' texts are written with them.
    keys: Option<KeyNames>,
    /// One bit per action whose state, shortcuts or texts changed since the last
    /// [`Registry::drain_changed`] (AM2, choice S3): bit `i % 64` of word `i / 64`.
    changed: Vec<u64>,
}

impl Registry {
    /// An empty registry for `platform`, with the *Global* context only.
    #[must_use]
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            contexts: vec!["global"],
            actions: Vec::new(),
            lookup: Lookup::default(),
            unknown: Vec::new(),
            keys: None,
            changed: Vec::new(),
        }
    }

    /// The platform.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// A new context named `name` (for developers and the shortcut editor: "notation", "daw").
    ///
    /// # Errors
    /// [`RegistryError::DuplicateContext`] or [`RegistryError::TooManyContexts`].
    pub fn add_context(&mut self, name: &'static str) -> Result<Context, RegistryError> {
        if self.contexts.contains(&name) {
            return Err(RegistryError::DuplicateContext(name));
        }
        let index =
            u8::try_from(self.contexts.len()).map_err(|_| RegistryError::TooManyContexts)?;
        self.contexts.push(name);
        Ok(Context(index))
    }

    /// Registers an action with its defaults for this registry's platform, enabled.
    ///
    /// # Errors
    /// A [`RegistryError`] when the identifier is malformed or taken, the context unknown, or a
    /// default is a fixed key or conflicts with another action's: the application's own tables
    /// are wrong, which its tests catch.
    pub fn register(&mut self, spec: ActionSpec) -> Result<ActionId, RegistryError> {
        if !well_formed(spec.identifier) {
            return Err(RegistryError::MalformedIdentifier(spec.identifier));
        }
        if self.find(spec.identifier).is_some() {
            return Err(RegistryError::DuplicateIdentifier(spec.identifier));
        }
        if usize::from(spec.context.0) >= self.contexts.len() {
            return Err(RegistryError::UnknownContext(spec.context));
        }
        let id =
            ActionId(u16::try_from(self.actions.len()).map_err(|_| RegistryError::TooManyActions)?);
        let defaults = deduplicated(spec.defaults.of(self.platform));
        self.check(id, spec.context, &defaults)?;
        for &shortcut in &defaults {
            self.lookup.insert((spec.context, shortcut), id);
        }
        self.actions.push(Entry {
            spec,
            shortcuts: defaults.clone(),
            defaults,
            state: ActionState::ENABLED,
            name_text: String::new(),
            shortcut_text: String::new(),
            mnemonic: None,
        });
        // The bit set grows with the actions, here and never while marking: one word per 64.
        if self.changed.len() * 64 < self.actions.len() {
            self.changed.push(0);
        }
        self.write_shortcut_text(id);
        Ok(id)
    }

    /// Gives the registry the texts people read (F2.11d2a): `name` turns each action's Fluent key
    /// ([`ActionSpec::name`]) into its name in the interface's language, and `keys` holds the
    /// keys' names for the shortcuts' texts. Called at start-up and when the language changes;
    /// every action is marked as changed (AM2), so open menus show the new texts.
    ///
    /// # Cost
    /// One call of `name` and one shortcut written per action; allocates the strings. Never on
    /// the path of a key or a frame.
    pub fn set_texts(&mut self, keys: KeyNames, mut name: impl FnMut(&'static str) -> String) {
        self.keys = Some(keys);
        for entry in &mut self.actions {
            entry.name_text = name(entry.spec.name);
        }
        for index in 0..self.actions.len() {
            if let Ok(index) = u16::try_from(index) {
                self.write_shortcut_text(ActionId(index));
            }
        }
    }

    /// Gives the registry the actions' **mnemonics** (F2.11d2b, ADR-0048 AM10): `mnemonic` turns
    /// each action's Fluent key into the letter its menu item is chosen with (the message's
    /// `.mnemonic` attribute), or `None`. The letter is kept in lower case, so a press matches it
    /// whatever the case. Called with [`Registry::set_texts`], when the language changes; every
    /// action whose letter changed is marked (AM2).
    pub fn set_mnemonics(&mut self, mut mnemonic: impl FnMut(&'static str) -> Option<char>) {
        for index in 0..self.actions.len() {
            let Some(entry) = self.actions.get_mut(index) else {
                continue;
            };
            let letter = mnemonic(entry.spec.name).map(lower_case);
            if entry.mnemonic != letter {
                entry.mnemonic = letter;
                if let Ok(index) = u16::try_from(index) {
                    self.mark(ActionId(index));
                }
            }
        }
    }

    /// The mnemonic of `id`'s menu item, in lower case (F2.11d2b): `None` without one, or for an
    /// unknown action.
    #[must_use]
    pub fn mnemonic(&self, id: ActionId) -> Option<char> {
        self.actions
            .get(id.index())
            .and_then(|entry| entry.mnemonic)
    }

    /// The name of `id` as people read it (empty until [`Registry::set_texts`], or for an unknown
    /// action).
    #[must_use]
    pub fn name_text(&self, id: ActionId) -> &str {
        self.actions
            .get(id.index())
            .map_or("", |entry| entry.name_text.as_str())
    }

    /// The first shortcut of `id` written for people, in the platform's style (F2.11d2a, choice
    /// S1): what a menu item shows at its right. Empty without a shortcut, until
    /// [`Registry::set_texts`], or for an unknown action.
    #[must_use]
    pub fn shortcut_text(&self, id: ActionId) -> &str {
        self.actions
            .get(id.index())
            .map_or("", |entry| entry.shortcut_text.as_str())
    }

    /// Calls `each` with every action marked as changed since the last call, in index order, and
    /// clears the marks (AM2, choice S3 of F2.11d2a).
    ///
    /// # Cost
    /// O(actions / 64 + marked): one word tested per 64 actions; no allocation.
    pub fn drain_changed(&mut self, mut each: impl FnMut(ActionId)) {
        for (word_index, word) in self.changed.iter_mut().enumerate() {
            let mut bits = std::mem::take(word);
            while bits != 0 {
                // The lowest set bit first, so the actions come in index order.
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                if let Ok(index) = u16::try_from(word_index * 64 + bit) {
                    each(ActionId(index));
                }
            }
        }
    }

    /// Marks `id` as changed (see [`Registry::drain_changed`]). O(1), no allocation: the bit set
    /// grew at registration.
    fn mark(&mut self, id: ActionId) {
        let index = id.index();
        if let Some(word) = self.changed.get_mut(index / 64) {
            *word |= 1 << (index % 64);
        }
    }

    /// Writes again the text of the first shortcut of `id` (when the texts are known) and marks
    /// the action as changed.
    fn write_shortcut_text(&mut self, id: ActionId) {
        let platform = self.platform;
        if let (Some(keys), Some(entry)) = (self.keys.as_ref(), self.actions.get_mut(id.index())) {
            entry.shortcut_text.clear();
            if let Some(first) = entry.shortcuts.first() {
                first.write_label(platform, keys, &mut entry.shortcut_text);
            }
        }
        self.mark(id);
    }

    /// The action whose identifier is `identifier`. O(n) in the number of actions: for loading
    /// the settings file and for tests, never while keys are handled.
    #[must_use]
    pub fn find(&self, identifier: &str) -> Option<ActionId> {
        self.actions
            .iter()
            .position(|entry| entry.spec.identifier == identifier)
            .and_then(|index| u16::try_from(index).ok())
            .map(ActionId)
    }

    /// How the application described `id`.
    #[must_use]
    pub fn spec(&self, id: ActionId) -> Option<&ActionSpec> {
        self.actions.get(id.index()).map(|entry| &entry.spec)
    }

    /// The shortcuts of `id` now (the defaults, or the user's). Empty for an unknown action.
    #[must_use]
    pub fn shortcuts(&self, id: ActionId) -> &[Shortcut] {
        self.actions
            .get(id.index())
            .map_or(&[], |entry| entry.shortcuts.as_slice())
    }

    /// The default shortcuts of `id` on this registry's platform.
    #[must_use]
    pub fn defaults(&self, id: ActionId) -> &[Shortcut] {
        self.actions
            .get(id.index())
            .map_or(&[], |entry| entry.defaults.as_slice())
    }

    /// The state of `id` (AM2).
    #[must_use]
    pub fn state(&self, id: ActionId) -> Option<ActionState> {
        self.actions.get(id.index()).map(|entry| entry.state)
    }

    /// Sets the state of `id`; `true` when it changed.
    ///
    /// # Errors
    /// [`RegistryError::UnknownAction`].
    pub fn set_state(&mut self, id: ActionId, state: ActionState) -> Result<bool, RegistryError> {
        let entry = self
            .actions
            .get_mut(id.index())
            .ok_or(RegistryError::UnknownAction(id))?;
        let changed = entry.state != state;
        entry.state = state;
        if changed {
            self.mark(id);
        }
        Ok(changed)
    }

    /// The action of `shortcut` in `context`, if any (one probe of the lookup, AM4).
    ///
    /// # Cost
    /// O(1) expected, no allocation.
    #[must_use]
    pub fn lookup(&self, context: Context, shortcut: Shortcut) -> Option<ActionId> {
        self.lookup.get(&(context, shortcut)).copied()
    }

    /// Replaces the shortcuts of `id` with `shortcuts` (duplicates dropped), the user's choice
    /// (AM7). All or nothing.
    ///
    /// # Errors
    /// [`RegistryError::UnknownAction`], [`RegistryError::FixedKey`], or
    /// [`RegistryError::Conflict`] when another action of the same context has one of them.
    pub fn set_shortcuts(
        &mut self,
        id: ActionId,
        shortcuts: &[Shortcut],
    ) -> Result<(), RegistryError> {
        let context = self
            .actions
            .get(id.index())
            .ok_or(RegistryError::UnknownAction(id))?
            .spec
            .context;
        let shortcuts = deduplicated(shortcuts);
        self.check(id, context, &shortcuts)?;
        self.replace(id, context, shortcuts);
        Ok(())
    }

    /// Brings back the defaults of `id`.
    ///
    /// # Errors
    /// [`RegistryError::UnknownAction`], or [`RegistryError::Conflict`] when the user gave one of
    /// the defaults to another action meanwhile (the shortcut editor asks which keeps it).
    pub fn restore(&mut self, id: ActionId) -> Result<(), RegistryError> {
        let entry = self
            .actions
            .get(id.index())
            .ok_or(RegistryError::UnknownAction(id))?;
        let (context, defaults) = (entry.spec.context, entry.defaults.clone());
        self.check(id, context, &defaults)?;
        self.replace(id, context, defaults);
        Ok(())
    }

    /// Brings back the defaults of every action (they never conflict: [`Registry::register`]
    /// checked them) and forgets the entries of unknown actions.
    pub fn restore_all(&mut self) {
        self.lookup.clear();
        self.unknown.clear();
        for (index, entry) in self.actions.iter_mut().enumerate() {
            entry.shortcuts.clone_from(&entry.defaults);
            let Ok(index) = u16::try_from(index) else {
                continue;
            };
            for &shortcut in &entry.shortcuts {
                self.lookup
                    .insert((entry.spec.context, shortcut), ActionId(index));
            }
        }
        // Every action's first shortcut may be another now: written again, and marked.
        for index in 0..self.actions.len() {
            if let Ok(index) = u16::try_from(index) {
                self.write_shortcut_text(ActionId(index));
            }
        }
    }

    /// The differences from the defaults, as the settings file stores them (AM7): each action
    /// whose shortcuts are not its defaults, by identifier, with its whole list (empty: "no
    /// shortcut"), in registration order; then the kept entries of unknown actions.
    pub fn overrides(&self) -> impl Iterator<Item = (&str, &[Shortcut])> + '_ {
        let known = self
            .actions
            .iter()
            .filter(|entry| entry.shortcuts != entry.defaults)
            .map(|entry| (entry.spec.identifier, entry.shortcuts.as_slice()));
        let unknown = self
            .unknown
            .iter()
            .map(|(identifier, shortcuts)| (identifier.as_str(), shortcuts.as_slice()));
        known.chain(unknown)
    }

    /// Starts from the defaults and applies the user's entries in order (AM7), as a settings file
    /// has them: the later entry wins a conflict (the earlier action loses that shortcut), fixed
    /// keys are dropped, unknown actions are kept and ignored. Returns what went wrong, for the
    /// application to log; nothing here stops the program.
    pub fn load_overrides(
        &mut self,
        entries: impl IntoIterator<Item = (String, Vec<Shortcut>)>,
    ) -> Vec<LoadWarning> {
        self.restore_all();
        let mut warnings = Vec::new();
        for (identifier, shortcuts) in entries {
            let Some(id) = self.find(&identifier) else {
                warnings.push(LoadWarning::UnknownAction(identifier.clone()));
                self.unknown.push((identifier, shortcuts));
                continue;
            };
            let Some(context) = self.spec(id).map(|spec| spec.context) else {
                continue;
            };
            let mut kept = Vec::with_capacity(shortcuts.len());
            for shortcut in deduplicated(&shortcuts) {
                if shortcut.is_fixed(self.platform) {
                    warnings.push(LoadWarning::FixedKey {
                        action: id,
                        shortcut,
                    });
                    continue;
                }
                // The later entry wins: the other action of the same context loses it.
                if let Some(&loser) = self.lookup.get(&(context, shortcut))
                    && loser != id
                {
                    self.take_from(loser, shortcut);
                    warnings.push(LoadWarning::Conflict {
                        shortcut,
                        winner: id,
                        loser,
                    });
                }
                kept.push(shortcut);
            }
            self.replace(id, context, kept);
        }
        warnings
    }

    /// Checks that `shortcuts` may belong to `id` in `context`: no fixed key, no other action of
    /// the context with one of them.
    fn check(
        &self,
        id: ActionId,
        context: Context,
        shortcuts: &[Shortcut],
    ) -> Result<(), RegistryError> {
        for &shortcut in shortcuts {
            if shortcut.is_fixed(self.platform) {
                return Err(RegistryError::FixedKey(shortcut));
            }
            if let Some(&owner) = self.lookup.get(&(context, shortcut))
                && owner != id
            {
                return Err(RegistryError::Conflict { shortcut, owner });
            }
        }
        Ok(())
    }

    /// Gives `id` the list `shortcuts`, already checked, updating the lookup: the old entries out,
    /// the new ones in. O(old + new).
    fn replace(&mut self, id: ActionId, context: Context, shortcuts: Vec<Shortcut>) {
        let Some(entry) = self.actions.get_mut(id.index()) else {
            return;
        };
        for shortcut in &entry.shortcuts {
            if self.lookup.get(&(context, *shortcut)) == Some(&id) {
                self.lookup.remove(&(context, *shortcut));
            }
        }
        for &shortcut in &shortcuts {
            self.lookup.insert((context, shortcut), id);
        }
        entry.shortcuts = shortcuts;
        self.write_shortcut_text(id);
    }

    /// Takes `shortcut` away from `id` (it lost a conflict while loading).
    fn take_from(&mut self, id: ActionId, shortcut: Shortcut) {
        let Some(entry) = self.actions.get_mut(id.index()) else {
            return;
        };
        entry.shortcuts.retain(|&own| own != shortcut);
        self.lookup.remove(&(entry.spec.context, shortcut));
        self.write_shortcut_text(id);
    }
}

/// The lower case of `letter` when it is a single character, otherwise `letter` itself (as the
/// shortcuts do, ADR-0048 AM5).
fn lower_case(letter: char) -> char {
    let mut lowered = letter.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => letter,
    }
}

/// `shortcuts` without repetitions, in their order (a short list: O(n²) on a handful).
fn deduplicated(shortcuts: &[Shortcut]) -> Vec<Shortcut> {
    let mut unique = Vec::with_capacity(shortcuts.len());
    for &shortcut in shortcuts {
        if !unique.contains(&shortcut) {
            unique.push(shortcut);
        }
    }
    unique
}

/// `true` for an identifier `group.name` (AM1): two non-empty parts of lower case ASCII letters,
/// digits and hyphens, separated by one dot, each starting with a letter.
fn well_formed(identifier: &str) -> bool {
    let part = |text: &str| {
        text.starts_with(|c: char| c.is_ascii_lowercase())
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    identifier
        .split_once('.')
        .is_some_and(|(group, name)| part(group) && part(name))
}

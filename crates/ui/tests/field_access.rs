// SPDX-License-Identifier: GPL-3.0-or-later

//! The text field's accessibility (ADR-0047 TF11, ADR-0045 WI1–WI4, step F2.10b3), headless.
//!
//! The tree the interface hands out is read here in two ways: as raw AccessKit nodes (what we
//! wrote), and through `accesskit_consumer`, the reader AccessKit's own platform adapters are
//! built on (what a screen reader gets: the text, the caret, the words, the rectangles). Then
//! the three requests an assistive technology can make (set the text, replace the selection,
//! select), with and without the focus, and a property test on random texts and selections.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use std::collections::HashMap;

use accesskit::{Action, Node, NodeId, Role, TreeId, TreeUpdate};
use accesskit_consumer::{NodeRef, Tree, TreeChangeHandler};
use orchestra_text::{Faces, grapheme_boundaries, word_starts};
use orchestra_ui::{
    AccessAction, AccessActionKind, AccessInfo, Appearance, BuiltInTheme, Button, ButtonState,
    Constraints, DisplayList, EditConvention, ImeEvent, InputEvent, Key, LayoutCx, LayoutProps,
    Modifiers, NamedKey, RUN_CHARACTERS, RecordCx, Recorder, Rect, Size, TextField, TextInfo,
    Theme, Ui, Widget, WidgetId,
};
use proptest::prelude::*;

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// The field committed this text.
    Committed(String),
    /// The field's text changed to this (live).
    Changed(String),
    /// The button after the field was pressed.
    Other,
}

/// The root of these tests: the field in a row of its own, then a button.
#[derive(Debug)]
struct Root;

/// The field's rectangle in the window, logical pixels.
const FIELD: Rect = Rect::new(10.0, 10.0, 210.0, 38.0);

impl Widget<Msg> for Root {
    fn measure(&mut self, cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        for child in 0..cx.child_count() {
            cx.measure_child(child, Constraints::tight(Size::new(FIELD.width(), 28.0)));
        }
        constraints.constrain(Size::new(400.0, 200.0))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, Msg>, _rect: Rect) {
        cx.place_child(0, FIELD);
        cx.place_child(1, Rect::new(10.0, 50.0, 210.0, 78.0));
    }

    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}
}

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// A window of 400 × 200.
const BOUNDS: Rect = Rect::new(0.0, 0.0, 400.0, 200.0);

/// A consumer of tree changes that ignores them: the tests look at the tree afterwards.
struct Ignore;

impl TreeChangeHandler for Ignore {
    fn node_added(&mut self, _node: &NodeRef) {}
    fn node_updated(&mut self, _old_node: &NodeRef, _new_node: &NodeRef) {}
    fn focus_moved(&mut self, _old_node: Option<&NodeRef>, _new_node: Option<&NodeRef>) {}
    fn node_removed(&mut self, _node: &NodeRef) {}
}

/// A test bench: the interface with a field and a button, and the tree a screen reader holds,
/// kept up to date with the interface's updates as the window's adapter would.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
    /// The reader's tree, built from the first (whole) update.
    tree: Tree,
    /// The last update handed out, to see what was sent.
    last: TreeUpdate,
    /// Every raw node by its number, as last sent (an update carries only what changed).
    known: HashMap<NodeId, Node>,
}

impl Bench {
    /// A field holding `text` (Linux keys, live changes on, a placeholder and a name).
    fn new(text: &str) -> Self {
        Self::with(text, true)
    }

    /// The same, enabled or not.
    fn with(text: &str, enabled: bool) -> Self {
        let mut field = TextField::new(text, |text| Msg::Committed(text.to_owned()))
            .on_change(|text| Msg::Changed(text.to_owned()))
            .placeholder("Name")
            .labelled("Track name")
            .convention(EditConvention::Linux);
        field.set_enabled(enabled);
        let mut ui = Ui::new(Root, &theme(), Faces::bundled().unwrap());
        let root = ui.root();
        let field = ui.add(root, field, LayoutProps::default()).unwrap();
        ui.add(root, Button::new("OK", Msg::Other), LayoutProps::default())
            .unwrap();
        let mut list = DisplayList::new();
        ui.layout(BOUNDS, &mut list);
        let last = ui.take_access_update().into_tree_update();
        let tree = Tree::new(last.clone(), true);
        let known = last.nodes.iter().cloned().collect();
        Self {
            ui,
            list,
            field,
            tree,
            last,
            known,
        }
    }

    /// Takes the interface's update and gives it to the reader's tree.
    fn sync(&mut self) {
        self.last = self.ui.take_access_update().into_tree_update();
        self.known.extend(self.last.nodes.iter().cloned());
        self.tree
            .update_and_process_changes(self.last.clone(), &mut Ignore);
    }

    /// Carries out an assistive technology's request on the field, then syncs.
    fn act(&mut self, kind: AccessActionKind) {
        self.ui
            .access_action(
                AccessAction {
                    target: self.field,
                    kind,
                },
                &mut self.list,
            )
            .unwrap();
        self.sync();
    }

    /// Presses and releases `key` with `modifiers`, then syncs.
    fn press(&mut self, key: &Key, modifiers: Modifiers) {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            self.ui.input(
                &InputEvent::Key {
                    key: key.clone(),
                    state,
                    repeat: false,
                    modifiers,
                },
                &mut self.list,
            );
        }
        self.sync();
    }

    /// Tab: the focus goes to the field (from nothing) or to the button (from the field).
    fn tab(&mut self) {
        self.press(&Key::Named(NamedKey::Tab), Modifiers::default());
    }

    /// The field widget.
    fn widget(&self) -> &TextField<Msg> {
        self.ui.widget::<TextField<Msg>>(self.field).unwrap()
    }

    /// The field's description of its text, as the widget gives it.
    fn info(&self) -> TextInfo {
        let mut info = AccessInfo::default();
        Widget::<Msg>::access_info(self.widget(), &mut info);
        info.text.unwrap()
    }

    /// The field's node in the reader's tree.
    fn node(&self) -> NodeRef<'_> {
        self.tree
            .state()
            .node_by_tree_local_id(NodeId(self.field.get()), TreeId::ROOT)
            .unwrap()
    }

    /// The raw node with this number, as last sent.
    fn raw(&self, id: NodeId) -> Option<&Node> {
        self.known.get(&id)
    }

    /// The raw node of the field, as last sent.
    fn raw_field(&self) -> &Node {
        self.raw(NodeId(self.field.get())).unwrap()
    }

    /// The raw run nodes of the field, as last sent, in order.
    fn raw_runs(&self) -> Vec<&Node> {
        self.raw_field()
            .children()
            .iter()
            .map(|&id| self.raw(id).unwrap())
            .collect()
    }

    /// The messages so far.
    fn messages(&mut self) -> Vec<Msg> {
        self.ui.take_messages().collect()
    }

    /// The selected text as the reader sees it.
    fn read_selection(&self) -> String {
        self.node().text_selection().unwrap().text()
    }

    /// The caret as the reader sees it: code points from the text's start.
    fn read_caret(&self) -> usize {
        self.node()
            .text_selection_focus()
            .unwrap()
            .to_global_usv_index()
    }
}

/// The number of the node of run `run` of the widget `id`: ADR-0045 WI2, written out by hand so
/// the test does not share the library's formula.
fn run_node(id: WidgetId, run: u64) -> NodeId {
    NodeId((1 << 63) | (run << 48) | id.get())
}

/// Ctrl (the primary modifier on Linux).
fn ctrl() -> Modifiers {
    Modifiers {
        primary: true,
        ..Modifiers::default()
    }
}

#[test]
fn a_reader_gets_the_name_the_text_the_caret_and_the_words() {
    let bench = Bench::new("Violin I, solo");
    let node = bench.node();
    assert_eq!(node.role(), Role::TextInput);
    assert_eq!(node.label().as_deref(), Some("Track name"));
    assert!(node.supports_text_ranges());
    assert_eq!(node.value().as_deref(), Some("Violin I, solo"));
    assert_eq!(node.document_range().text(), "Violin I, solo");
    // A new field has its caret at the end and nothing selected.
    assert!(node.text_selection().unwrap().is_degenerate());
    assert_eq!(bench.read_caret(), 14);
    // "Next word" from the start stops where Ctrl+→ does on Windows (`next_word_start`): the
    // comma and the spaces belong to the word before them.
    let mut position = node.document_start();
    let mut stops = Vec::new();
    while !position.is_document_end() {
        position = position.forward_to_word_start();
        stops.push(position.to_global_usv_index());
    }
    assert_eq!(stops, vec![7, 10, 14]);
    // The placeholder is announced only on an empty field.
    assert_eq!(node.placeholder(), None);
    // One run for a short text, with the number ADR-0045 WI2 gives it.
    assert_eq!(bench.raw_field().children(), &[run_node(bench.field, 0)]);
    let run = bench.raw_runs()[0];
    assert_eq!(run.role(), Role::TextRun);
    assert_eq!(run.character_lengths(), &[1; 14]);
    assert_eq!(run.word_starts(), &[0, 7, 10]);
}

#[test]
fn the_characters_rectangles_are_where_the_text_is_drawn() {
    let bench = Bench::new("Violin");
    let info = bench.info();
    // Six letters one after the other, each as wide as the step to the next.
    assert_eq!(info.character_positions.len(), 6);
    assert!(info.character_positions[0].abs() < 0.01);
    for pair in 0..5 {
        let end = info.character_positions[pair] + info.character_widths[pair];
        assert!((end - info.character_positions[pair + 1]).abs() < 0.01);
        assert!(info.character_widths[pair] > 1.0);
    }
    // The line is inside the field, after its padding, and as high as the line of text.
    assert!(info.rect.x0 > FIELD.x0 && info.rect.x1 < FIELD.x1);
    assert!(info.rect.y0 >= FIELD.y0 && info.rect.y1 <= FIELD.y1);
    // The reader's rectangle of the whole text is the line's.
    let boxes = bench.node().document_range().bounding_boxes();
    assert_eq!(boxes.len(), 1);
    assert!((boxes[0].x0 - f64::from(info.rect.x0)).abs() < 0.01);
    assert!((boxes[0].x1 - f64::from(info.rect.x1)).abs() < 0.01);
    assert!((boxes[0].y0 - f64::from(info.rect.y0)).abs() < 0.01);
    assert!((boxes[0].y1 - f64::from(info.rect.y1)).abs() < 0.01);
}

#[test]
fn an_empty_field_has_one_empty_run_and_announces_its_placeholder() {
    let bench = Bench::new("");
    let node = bench.node();
    assert!(node.supports_text_ranges());
    assert_eq!(node.value().as_deref(), Some(""));
    assert_eq!(node.placeholder(), Some("Name"));
    assert_eq!(bench.read_caret(), 0);
    let runs = bench.raw_runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].value(), Some(""));
    assert_eq!(runs[0].character_lengths(), [0_u8; 0]);
}

#[test]
fn accents_and_flags_are_one_character_each() {
    // "e" + U+0301 (3 bytes), "x", the Italian flag (two regional indicators, 8 bytes), "y".
    let mut bench = Bench::new("e\u{301}x\u{1F1EE}\u{1F1F9}y");
    assert_eq!(bench.raw_runs()[0].character_lengths(), &[3, 1, 8, 1]);
    // Select from character 1 to character 3: "x" and the flag, bytes 3 to 12.
    bench.act(AccessActionKind::SetTextSelection {
        anchor: 1,
        focus: 3,
    });
    assert_eq!(bench.widget().selection(), 3..12);
    assert_eq!(bench.widget().caret(), 12);
    assert_eq!(bench.read_selection(), "x\u{1F1EE}\u{1F1F9}");
    // Backwards: the caret is at the start.
    bench.act(AccessActionKind::SetTextSelection {
        anchor: 3,
        focus: 0,
    });
    assert_eq!(bench.widget().selection(), 0..12);
    assert_eq!(bench.widget().caret(), 0);
    // Past the end: the end.
    bench.act(AccessActionKind::SetTextSelection {
        anchor: 99,
        focus: 99,
    });
    assert_eq!(bench.widget().caret(), 13);
    // The reader counts code points: e, the accent, x, the flag's two halves, y.
    assert_eq!(bench.read_caret(), 6);
}

#[test]
fn a_long_text_is_several_runs_of_one_line() {
    // 600 characters: runs of 255, 255 and 90.
    let text = "ab cd ".repeat(100);
    let mut bench = Bench::new(&text);
    let runs = bench.raw_runs();
    assert_eq!(runs.len(), 3);
    let lengths: Vec<usize> = runs
        .iter()
        .map(|run| run.character_lengths().len())
        .collect();
    assert_eq!(lengths, vec![RUN_CHARACTERS, RUN_CHARACTERS, 90]);
    let ids: Vec<NodeId> = (0..3).map(|run| run_node(bench.field, run)).collect();
    assert_eq!(bench.raw_field().children(), ids.as_slice());
    // Linked as one line.
    assert_eq!(runs[0].previous_on_line(), None);
    assert_eq!(runs[0].next_on_line(), Some(ids[1]));
    assert_eq!(runs[1].previous_on_line(), Some(ids[0]));
    assert_eq!(runs[1].next_on_line(), Some(ids[2]));
    assert_eq!(runs[2].previous_on_line(), Some(ids[1]));
    assert_eq!(runs[2].next_on_line(), None);
    // Each run's rectangle starts where the previous one ends, and its positions from 0.
    for pair in runs.windows(2) {
        let (before, after) = (pair[0].bounds().unwrap(), pair[1].bounds().unwrap());
        assert!((before.x1 - after.x0).abs() < 0.01);
        assert!(pair[1].character_positions().unwrap()[0].abs() < 0.001);
    }
    // Word starts inside each run count from the run's own first character: 255 = 6 × 42 + 3,
    // so the second run starts with "cd " and its first words start at 0 and 3.
    assert_eq!(&runs[1].word_starts()[..3], &[0, 3, 6]);
    // The reader sees one text and one line, and walks the words across the runs.
    let node = bench.node();
    assert_eq!(node.document_range().text(), text);
    let mut position = node.document_start();
    let mut stops = vec![0];
    while !position.is_document_end() {
        position = position.forward_to_word_start();
        stops.push(position.to_global_usv_index());
    }
    let mut expected: Vec<usize> = word_starts(&text).collect();
    expected.push(text.len());
    assert_eq!(stops, expected);
    assert!(
        node.document_start()
            .forward_to_line_end()
            .is_document_end()
    );
    // The caret on the border of two runs, in the second run, and at the very end.
    for (character, run, index) in [(255, 1, 0), (300, 1, 45), (510, 2, 0), (600, 2, 90)] {
        bench.act(AccessActionKind::SetTextSelection {
            anchor: character,
            focus: character,
        });
        assert_eq!(bench.widget().caret(), character);
        let focus = bench.raw_field().text_selection().unwrap().focus;
        assert_eq!(
            (focus.node, focus.character_index),
            (ids[run], index),
            "{character}"
        );
        assert_eq!(bench.read_caret(), character);
    }
}

#[test]
fn a_text_that_shrinks_loses_its_extra_runs() {
    let mut bench = Bench::new(&"ab cd ".repeat(100));
    assert_eq!(bench.node().children().count(), 3);
    bench.act(AccessActionKind::SetText("Viola".to_owned()));
    assert_eq!(bench.raw_runs().len(), 1);
    assert_eq!(bench.node().children().count(), 1);
    assert_eq!(bench.node().value().as_deref(), Some("Viola"));
    // The old runs are gone from the reader's tree.
    for run in 1..3 {
        assert!(
            bench
                .tree
                .state()
                .node_by_tree_local_id(run_node(bench.field, run), TreeId::ROOT)
                .is_none()
        );
    }
}

#[test]
fn a_grapheme_longer_than_255_bytes_is_given_in_pieces() {
    // "a" under 300 combining acute accents (2 bytes each): one grapheme of 601 bytes, then "b".
    let text = format!("a{}b", "\u{301}".repeat(300));
    let mut bench = Bench::new(&text);
    let info = bench.info();
    // 255 bytes ("a" and 127 accents), 254 (127 accents), 92 (46 accents), then "b".
    assert_eq!(info.character_lengths, vec![255, 254, 92, 1]);
    // The first piece has the grapheme's width, the others none, at its end.
    assert!(info.character_widths[0] > 1.0);
    assert!(info.character_widths[1].abs() < f32::EPSILON);
    assert!(info.character_widths[2].abs() < f32::EPSILON);
    assert!((info.character_positions[1] - info.character_positions[3]).abs() < 0.01);
    assert_eq!(bench.node().document_range().text(), text);
    // The caret is after everything: character 4.
    assert_eq!(info.focus, 4);
    // A selection inside the grapheme goes to the boundary after it: byte 601, character 3.
    bench.act(AccessActionKind::SetTextSelection {
        anchor: 1,
        focus: 1,
    });
    assert_eq!(bench.widget().caret(), 601);
    assert_eq!(bench.info().focus, 3);
}

#[test]
fn a_scrolled_text_reaches_outside_the_field_which_clips_it() {
    let mut bench = Bench::new(&"wide ".repeat(40));
    assert!(bench.node().clips_children());
    // Unfocused, the caret (at the end) is in view: the text is scrolled to the left.
    let info = bench.info();
    assert!(info.rect.x0 < FIELD.x0);
    assert!(info.rect.x1 <= FIELD.x1);
    let run = bench.raw_runs()[0].bounds().unwrap();
    assert!(run.x0 < f64::from(FIELD.x0));
    // Moving the caret to the start scrolls back, and the new rectangles are sent.
    bench.act(AccessActionKind::SetTextSelection {
        anchor: 0,
        focus: 0,
    });
    let info = bench.info();
    assert!(info.rect.x0 > FIELD.x0);
    assert!(info.rect.x1 > FIELD.x1);
    let run = bench.raw_runs()[0].bounds().unwrap();
    assert!((run.x0 - f64::from(info.rect.x0)).abs() < 0.01);
}

#[test]
fn with_the_focus_a_set_text_is_an_edit_committed_later() {
    let mut bench = Bench::new("Violin");
    bench.tab();
    assert_eq!(bench.ui.focused(), Some(bench.field));
    let _ = bench.messages();
    // Cleaned as a paste: the line break becomes a space.
    bench.act(AccessActionKind::SetText("Cello\nsolo".to_owned()));
    assert_eq!(bench.widget().text(), "Cello solo");
    assert_eq!(bench.node().value().as_deref(), Some("Cello solo"));
    assert_eq!(bench.read_caret(), 10);
    // The live listener hears of it; nothing is committed yet.
    assert_eq!(
        bench.messages(),
        vec![Msg::Changed("Cello solo".to_owned())]
    );
    // One undo step takes it back.
    bench.press(&Key::Character("z".to_owned()), ctrl());
    assert_eq!(bench.widget().text(), "Violin");
    assert_eq!(bench.node().value().as_deref(), Some("Violin"));
    bench.press(
        &Key::Character("z".to_owned()),
        Modifiers {
            shift: true,
            ..ctrl()
        },
    );
    assert_eq!(bench.widget().text(), "Cello solo");
    let _ = bench.messages();
    // The focus leaving commits it, once.
    bench.tab();
    assert_eq!(
        bench.messages(),
        vec![Msg::Committed("Cello solo".to_owned())]
    );
}

#[test]
fn without_the_focus_a_changed_text_is_committed_at_once() {
    let mut bench = Bench::new("Violin");
    assert_eq!(bench.ui.focused(), None);
    bench.act(AccessActionKind::SetText("Cello".to_owned()));
    assert_eq!(bench.widget().text(), "Cello");
    assert_eq!(
        bench.messages(),
        vec![
            Msg::Changed("Cello".to_owned()),
            Msg::Committed("Cello".to_owned())
        ]
    );
    // The local history ended with the commit, as when the focus leaves.
    assert!(!bench.widget().editor().can_undo());
    // The same text again changes nothing worth committing.
    bench.act(AccessActionKind::SetText("Cello".to_owned()));
    assert!(
        !bench
            .messages()
            .iter()
            .any(|message| matches!(message, Msg::Committed(_)))
    );
    // Replacing the selection (none: the caret is at the end) inserts there, committed too.
    bench.act(AccessActionKind::ReplaceSelectedText(" II".to_owned()));
    assert_eq!(bench.widget().text(), "Cello II");
    assert_eq!(
        bench.messages().last(),
        Some(&Msg::Committed("Cello II".to_owned()))
    );
    // A selection alone commits nothing.
    bench.act(AccessActionKind::SetTextSelection {
        anchor: 0,
        focus: 5,
    });
    assert_eq!(bench.messages(), vec![]);
    assert_eq!(bench.read_selection(), "Cello");
    // And the selected text can then be replaced.
    bench.act(AccessActionKind::ReplaceSelectedText("Viola".to_owned()));
    assert_eq!(bench.widget().text(), "Viola II");
    assert_eq!(bench.node().value().as_deref(), Some("Viola II"));
}

#[test]
fn the_text_actions_are_offered_by_an_enabled_field_only() {
    let text_actions = [
        Action::SetValue,
        Action::ReplaceSelectedText,
        Action::SetTextSelection,
    ];
    let enabled = Bench::new("Violin");
    for action in text_actions {
        assert!(enabled.raw_field().supports_action(action), "{action:?}");
    }
    let mut disabled = Bench::with("Violin", false);
    for action in text_actions {
        assert!(!disabled.raw_field().supports_action(action), "{action:?}");
    }
    // Its text is still readable.
    assert!(disabled.node().is_disabled());
    assert_eq!(disabled.node().value().as_deref(), Some("Violin"));
    // A request that comes anyway changes nothing.
    disabled.act(AccessActionKind::SetText("Cello".to_owned()));
    assert_eq!(disabled.widget().text(), "Violin");
    assert_eq!(disabled.messages(), vec![]);
}

#[test]
fn typing_sends_the_field_and_its_runs_again_and_nothing_else() {
    let mut bench = Bench::new("Violin");
    bench.tab();
    bench
        .ui
        .input(&InputEvent::Text("!".to_owned()), &mut bench.list);
    bench.sync();
    let sent: Vec<NodeId> = bench.last.nodes.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        sent,
        vec![NodeId(bench.field.get()), run_node(bench.field, 0)]
    );
    // Tab selected everything, so the typed character replaced it.
    assert_eq!(bench.node().value().as_deref(), Some("!"));
    assert_eq!(bench.read_caret(), 1);
}

/// A text of up to 300 pieces drawn from letters, digits, spaces, punctuation, an accented
/// letter in both forms, a combining accent alone, a flag's halves, an emoji with a modifier and
/// a joiner: so graphemes of several code points, and texts on both sides of one run.
fn any_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("a"),
        Just("B"),
        Just("7"),
        Just(" "),
        Just(","),
        Just("-"),
        Just("é"),
        Just("e\u{301}"),
        Just("\u{301}"),
        Just("\u{1F1EE}"),
        Just("\u{1F1F9}"),
        Just("\u{1F44D}\u{1F3FD}"),
        Just("\u{200D}"),
    ];
    proptest::collection::vec(piece, 0..300).prop_map(|pieces| pieces.concat())
}

/// A composition of the input method (F2.10c2, choice D5) is described as it is shown: in the
/// value, in the runs and in the reader's document, with the caret where the input method puts
/// it; when it ends, the text alone again.
#[test]
fn a_composition_is_described_as_shown_with_its_caret() {
    let mut bench = Bench::new("ab");
    bench.tab();
    bench.press(&Key::Named(NamedKey::End), Modifiers::default());
    bench.ui.input(
        &InputEvent::Ime(ImeEvent::Preedit {
            text: "nihao".to_owned(),
            cursor: Some(2..2),
        }),
        &mut bench.list,
    );
    bench.sync();
    let info = bench.info();
    assert_eq!(info.value, "abnihao");
    // "ab" and "ni" before the input method's caret: character 4, no selection.
    assert_eq!((info.anchor, info.focus), (4, 4));
    assert_eq!(bench.node().document_range().text(), "abnihao");
    assert_eq!(bench.read_caret(), 4);
    bench.ui.input(
        &InputEvent::Ime(ImeEvent::Preedit {
            text: String::new(),
            cursor: None,
        }),
        &mut bench.list,
    );
    bench.sync();
    assert_eq!(bench.info().value, "ab");
    assert_eq!(bench.node().document_range().text(), "ab");
    assert_eq!(bench.read_caret(), 2);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// For any text and any selection: the runs describe the text exactly (their values joined
    /// are the text, their lengths are its graphemes, no run is longer than allowed), the reader
    /// sees the same text and the same selection, and asking for the selection the description
    /// names gives it back.
    #[test]
    fn any_text_and_selection_are_described_exactly(
        text in any_text(),
        anchor in 0_usize..400,
        focus in 0_usize..400,
    ) {
        let mut bench = Bench::new(&text);
        // The editor cleans its text (here nothing to clean, but it is the reference).
        let text = bench.widget().text().to_owned();
        let boundaries: Vec<usize> = grapheme_boundaries(&text).collect();
        let graphemes = boundaries.len() - 1;
        bench.act(AccessActionKind::SetTextSelection { anchor, focus });
        // The editor is where the characters asked for are (past the end: the end).
        let byte = |character: usize| boundaries[character.min(graphemes)];
        prop_assert_eq!(bench.widget().caret(), byte(focus));
        let selection = byte(anchor).min(byte(focus))..byte(anchor).max(byte(focus));
        prop_assert_eq!(bench.widget().selection(), selection.clone());
        // The description: one character per grapheme (none here is over 255 bytes).
        let info = bench.info();
        let lengths: Vec<usize> = boundaries.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let described: Vec<usize> =
            info.character_lengths.iter().map(|&length| usize::from(length)).collect();
        prop_assert_eq!(&described, &lengths);
        prop_assert_eq!(info.anchor, anchor.min(graphemes));
        prop_assert_eq!(info.focus, focus.min(graphemes));
        // The word starts are the editing keys' ones, as characters: each is the first character
        // at or after it (a word may start inside a grapheme, after a joiner: two starts can
        // then be the same character, listed once).
        let mut words: Vec<usize> = word_starts(&text)
            .map(|start| boundaries.iter().position(|&boundary| boundary >= start).unwrap())
            .collect();
        words.dedup();
        prop_assert_eq!(&info.word_starts, &words);
        // The runs: joined they are the text, none too long, lengths matching values.
        let runs = bench.raw_runs();
        prop_assert_eq!(runs.len(), graphemes.div_ceil(RUN_CHARACTERS).max(1));
        let mut joined = String::new();
        for run in &runs {
            let value = run.value().unwrap();
            prop_assert!(run.character_lengths().len() <= RUN_CHARACTERS);
            let bytes: usize = run.character_lengths().iter().map(|&l| usize::from(l)).sum();
            prop_assert_eq!(bytes, value.len());
            prop_assert_eq!(run.character_positions().unwrap().len(), run.character_lengths().len());
            prop_assert_eq!(run.character_widths().unwrap().len(), run.character_lengths().len());
            joined.push_str(value);
        }
        prop_assert_eq!(&joined, &text);
        // The reader: the same text, the same selected text.
        prop_assert_eq!(bench.node().document_range().text(), text.clone());
        prop_assert_eq!(bench.read_selection(), text.get(selection).unwrap());
    }
}

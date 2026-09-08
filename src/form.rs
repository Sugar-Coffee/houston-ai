//! Modal forms: a few labelled fields, one focused at a time.
//!
//! Shared by the new-session dialog and the settings menu, so directory
//! completion and toggling behave identically wherever you meet them.

use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    /// Offers `Tab` completion over directories.
    Directory,
    /// Flipped with space or Return.
    Toggle,
    /// One of a fixed list. Return cycles to the next.
    ///
    /// A cycle rather than a popup: the lists here are short, and seeing the
    /// result immediately is the point — you pick a theme by looking at it.
    Choice,
    /// A button. Return on it submits the form.
    ///
    /// A row rather than a modifier chord because most terminals send Ctrl-Enter
    /// as a plain Return — a "press Ctrl-Enter to create" binding would simply
    /// not exist for most people. A visible row also says the form is finishable
    /// without your having to know a key.
    Action,
}

/// What pressing Return on a field did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// The field now has the keyboard.
    Editing,
    /// A toggle flipped; nothing else changed.
    Toggled,
    /// The form should be accepted.
    Submitted,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub label: &'static str,
    /// Shown dim beside the field when it is empty or focused.
    pub hint: &'static str,
    pub kind: FieldKind,
    pub value: String,
    pub on: bool,
    /// Candidates from the last completion attempt, shown under the field.
    pub completions: Vec<String>,
    /// Hidden until a condition holds — the worktree name is pointless until
    /// the worktree toggle is on.
    pub visible: bool,
    /// The options a [`FieldKind::Choice`] cycles through.
    pub options: Vec<String>,
}

impl Field {
    pub fn text(label: &'static str, hint: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            hint,
            kind: FieldKind::Text,
            value: value.into(),
            on: false,
            completions: Vec::new(),
            visible: true,
            options: Vec::new(),
        }
    }

    pub fn directory(label: &'static str, hint: &'static str, value: impl Into<String>) -> Self {
        Self { kind: FieldKind::Directory, ..Self::text(label, hint, value) }
    }

    pub fn toggle(label: &'static str, hint: &'static str, on: bool) -> Self {
        Self { kind: FieldKind::Toggle, on, ..Self::text(label, hint, "") }
    }

    pub fn action(label: &'static str, hint: &'static str) -> Self {
        Self { kind: FieldKind::Action, ..Self::text(label, hint, "") }
    }

    pub fn choice(
        label: &'static str,
        hint: &'static str,
        options: Vec<String>,
        current: &str,
    ) -> Self {
        let value = options
            .iter()
            .find(|option| option.as_str() == current)
            .cloned()
            .or_else(|| options.first().cloned())
            .unwrap_or_default();

        Self { kind: FieldKind::Choice, options, ..Self::text(label, hint, value) }
    }

    /// Moves a choice field to its next option, wrapping.
    fn cycle(&mut self) {
        if self.options.is_empty() {
            return;
        }
        let next = self
            .options
            .iter()
            .position(|option| *option == self.value)
            .map_or(0, |index| (index + 1) % self.options.len());
        self.value = self.options[next].clone();
    }

    /// What the field shows when it is not being edited.
    pub fn display(&self) -> String {
        match self.kind {
            FieldKind::Toggle => if self.on { "yes" } else { "no" }.to_string(),
            FieldKind::Action => self.hint.to_string(),
            FieldKind::Choice => self.value.clone(),
            _ if self.value.is_empty() => self.hint.to_string(),
            _ => self.value.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Form {
    pub fields: Vec<Field>,
    focused: usize,
    /// `true` once a field is being typed into, as opposed to being selected.
    ///
    /// The two-step — move, then Return to edit — is what makes this a menu
    /// rather than a wall of always-live text boxes.
    editing: bool,
}

impl Form {
    #[must_use]
    pub fn new(fields: Vec<Field>) -> Self {
        let mut form = Self { fields, focused: 0, editing: false };
        form.focus_first_visible();
        form
    }

    pub const fn is_editing(&self) -> bool {
        self.editing
    }

    pub const fn focused_index(&self) -> usize {
        self.focused
    }

    pub fn focused(&self) -> Option<&Field> {
        self.fields.get(self.focused)
    }

    pub fn field(&self, label: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.label == label)
    }

    pub fn value(&self, label: &str) -> String {
        self.field(label).map(Field::display).unwrap_or_default()
    }

    pub fn is_on(&self, label: &str) -> bool {
        self.field(label).is_some_and(|field| field.on)
    }

    pub fn set_visible(&mut self, label: &str, visible: bool) {
        if let Some(field) = self.fields.iter_mut().find(|field| field.label == label) {
            field.visible = visible;
        }
        if !self.focused().is_some_and(|field| field.visible) {
            self.focus_first_visible();
        }
    }

    fn focus_first_visible(&mut self) {
        if let Some(index) = self.fields.iter().position(|field| field.visible) {
            self.focused = index;
        }
    }

    /// Moves between fields, skipping hidden ones.
    pub fn move_focus(&mut self, forward: bool) {
        let count = self.fields.len();
        if count == 0 {
            return;
        }

        for step in 1..=count {
            let next = if forward {
                (self.focused + step) % count
            } else {
                (self.focused + count - step % count) % count
            };
            if self.fields[next].visible {
                self.focused = next;
                return;
            }
        }
    }

    /// Return on a field: submits, flips a toggle, or begins editing.
    pub fn activate(&mut self) -> Activation {
        let Some(field) = self.fields.get_mut(self.focused) else { return Activation::Toggled };

        match field.kind {
            FieldKind::Action => Activation::Submitted,
            FieldKind::Choice => {
                field.cycle();
                Activation::Toggled
            }
            FieldKind::Toggle => {
                field.on = !field.on;
                Activation::Toggled
            }
            FieldKind::Text | FieldKind::Directory => {
                self.editing = true;
                Activation::Editing
            }
        }
    }

    /// Finishes editing, keeping what was typed.
    pub fn commit_field(&mut self) {
        self.editing = false;
        if let Some(field) = self.fields.get_mut(self.focused) {
            field.completions.clear();
        }
    }

    pub fn push(&mut self, character: char) {
        if let Some(field) = self.fields.get_mut(self.focused) {
            field.value.push(character);
            field.completions.clear();
        }
    }

    pub fn pop(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focused) {
            field.value.pop();
            field.completions.clear();
        }
    }

    /// `Tab` on a directory field: extend as far as the candidates agree.
    pub fn complete(&mut self) {
        let Some(field) = self.fields.get_mut(self.focused) else { return };
        if field.kind != FieldKind::Directory {
            return;
        }

        let completion = paths::complete_directory(&field.value);
        field.value = completion.extended;
        // Only worth listing when there is a choice left to make.
        field.completions =
            if completion.matches.len() > 1 { completion.matches } else { Vec::new() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form::new(vec![
            Field::text("Name", "unnamed", ""),
            Field::directory("Directory", "~/", "~/"),
            Field::toggle("Worktree", "run in an isolated checkout", false),
            Field::text("Worktree name", "from the session name", ""),
        ])
    }

    #[test]
    fn focus_moves_between_fields_and_wraps() {
        let mut form = form();
        assert_eq!(form.focused_index(), 0);

        form.move_focus(true);
        assert_eq!(form.focused().unwrap().label, "Directory");

        form.move_focus(false);
        assert_eq!(form.focused().unwrap().label, "Name");

        form.move_focus(false);
        assert_eq!(form.focused().unwrap().label, "Worktree name", "wraps to the end");
    }

    #[test]
    fn hidden_fields_are_skipped_and_never_left_focused() {
        let mut form = form();
        form.set_visible("Worktree name", false);

        form.move_focus(false);
        assert_eq!(form.focused().unwrap().label, "Worktree", "the hidden field is skipped");

        // Hiding the focused field moves focus somewhere real.
        form.set_visible("Worktree", false);
        assert!(form.focused().unwrap().visible);
    }

    #[test]
    fn editing_is_a_deliberate_second_step() {
        let mut form = form();
        assert!(!form.is_editing(), "a form opens as a menu, not a wall of text boxes");

        assert_eq!(form.activate(), Activation::Editing);
        assert!(form.is_editing());

        form.push('a');
        assert_eq!(form.value("Name"), "a");

        form.commit_field();
        assert!(!form.is_editing());
    }

    #[test]
    fn return_on_a_toggle_flips_it_rather_than_editing() {
        let mut form = form();
        form.move_focus(true);
        form.move_focus(true);
        assert_eq!(form.focused().unwrap().label, "Worktree");

        assert_eq!(form.activate(), Activation::Toggled, "a toggle never takes the keyboard");
        assert!(form.is_on("Worktree"));
        assert!(!form.is_editing());

        form.activate();
        assert!(!form.is_on("Worktree"), "and flips back");
    }

    #[test]
    fn an_empty_field_displays_its_hint() {
        let form = form();
        assert_eq!(form.value("Name"), "unnamed", "the hint stands in for an empty value");
        assert_eq!(form.value("Directory"), "~/");
    }

    #[test]
    fn a_toggle_reads_as_yes_or_no() {
        let mut form = form();
        assert_eq!(form.value("Worktree"), "no");

        form.move_focus(true);
        form.move_focus(true);
        form.activate();
        assert_eq!(form.value("Worktree"), "yes");
    }

    #[test]
    fn a_choice_cycles_through_its_options_and_wraps() {
        let options = vec!["One".to_string(), "Two".to_string(), "Three".to_string()];
        let mut form = Form::new(vec![Field::choice("Theme", "", options, "Two")]);

        assert_eq!(form.value("Theme"), "Two", "it opens on the current value");

        assert_eq!(form.activate(), Activation::Toggled, "a choice never takes the keyboard");
        assert_eq!(form.value("Theme"), "Three");

        form.activate();
        assert_eq!(form.value("Theme"), "One", "and wraps");
    }

    #[test]
    fn a_choice_falls_back_to_the_first_option_when_the_current_is_unknown() {
        let options = vec!["One".to_string(), "Two".to_string()];
        let form = Form::new(vec![Field::choice("Theme", "", options, "Deleted")]);

        assert_eq!(form.value("Theme"), "One", "a theme file that vanished must not blank the row");
    }

    /// Ctrl-Enter is indistinguishable from Enter in most terminals, so the
    /// form is finished by a visible row instead.
    #[test]
    fn an_action_row_submits_the_form() {
        let mut form = Form::new(vec![
            Field::text("Name", "", ""),
            Field::action("Create", "start the session"),
        ]);

        form.move_focus(true);
        assert_eq!(form.focused().unwrap().label, "Create");
        assert_eq!(form.activate(), Activation::Submitted);
        assert!(!form.is_editing(), "submitting never takes the keyboard");
    }

    #[test]
    fn tab_completes_a_directory_field_but_not_a_text_one() {
        let mut form = form();

        // Text field: Tab does nothing.
        form.activate();
        form.push('/');
        form.push('u');
        form.push('s');
        form.complete();
        assert_eq!(form.value("Name"), "/us", "a text field is left alone");

        form.commit_field();
        form.move_focus(true);
        form.activate();
        for _ in 0..2 {
            form.pop();
        }
        for character in "/usr/lo".chars() {
            form.push(character);
        }
        form.complete();
        assert_eq!(form.value("Directory"), "/usr/local/");
    }

    #[test]
    fn completions_are_listed_only_when_there_is_a_choice() {
        let root = std::env::temp_dir().join("houston-form-complete");
        let _ = std::fs::remove_dir_all(&root);
        for name in ["alpha", "alpine"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }

        let mut form =
            Form::new(vec![Field::directory("Directory", "", format!("{}/al", root.display()))]);
        form.activate();
        form.complete();

        let field = form.focused().unwrap();
        assert_eq!(field.completions, vec!["alpha", "alpine"], "ambiguous, so list them");

        form.push('p');
        form.push('h');
        form.complete();
        assert!(
            form.focused().unwrap().completions.is_empty(),
            "unambiguous, so nothing to choose between"
        );

        std::fs::remove_dir_all(&root).ok();
    }
}

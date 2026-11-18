use std::collections::HashMap;
use std::rc::Rc;

use std::sync::Arc;

use ala::{
    AnyElement, AppContext as _, ClickEvent, Context, ElementId, Entity, EventEmitter,
    FontFeatures, FontWeight, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, div,
};
use serde_json::Value;

use super::catalog::{self as spec, Align, Dynamic, Justify, Kind, Scalar, Words};
use super::document::{Document, Message, absolute};
use crate::blocks::touch::CoarsePointer;
use crate::components::badge::Badge;
use crate::components::button::{Button, ButtonVariants as _};
use crate::components::chart::{Chart, ChartKind, Series};
use crate::components::field::Field;
use crate::components::input::{Input, InputEvent, InputState};
use crate::components::progress::Progress;
use crate::components::segmented::SegmentedControl;
use crate::components::select::{Select, SelectEvent, SelectItem, SelectState};
use crate::components::separator::Separator;
use crate::components::slider::Slider;
use crate::{
    ActiveTheme as _, Card, CardLook, Disableable as _, IconName, IndexPath, Label, Sizable as _,
    Size, Space, StyledExt as _, TextStyle, Tone, h_flex, rems_from_px, v_flex,
};

const MAX_DEPTH: usize = 32;
const FIELD_WIDTH: f32 = 320.0;
const TOUCH_BUTTON_WIDTH: f32 = 120.0;
const TOUCH_COLUMNS: u16 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub name: String,
    pub surface_id: String,
    pub source_component_id: String,
    pub context: Vec<(String, Value)>,
    pub user_message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Live,
    Held {
        pressed: Option<String>,
    },
}

#[derive(Clone, Copy)]
struct Place<'base> {
    base: &'base str,
    depth: usize,
    dense: bool,
}

impl<'base> Place<'base> {
    fn deeper(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }

    fn at(self, base: &'base str) -> Self {
        Self { base, ..self }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum A2uiEvent {
    Action(Action),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Model(String),
    Local(String),
}

impl Slot {
    fn of<T>(value: &Dynamic<T>, key: &str, base: &str) -> Self {
        match value {
            Dynamic::Path { path } => Self::Model(absolute(base, path)),
            Dynamic::Literal(_) => Self::Local(key.to_owned()),
        }
    }
}

impl SelectItem for spec::Choice {
    type Value = String;

    fn title(&self) -> SharedString {
        SharedString::from(self.label().to_owned())
    }

    fn value(&self) -> &Self::Value {
        self.key()
    }
}

pub struct A2uiSurface {
    id: SharedString,
    document: Rc<Document>,
    mode: Mode,
    touch: bool,
    locals: HashMap<String, Value>,
    inputs: HashMap<String, Entity<InputState>>,
    selects: HashMap<String, Entity<SelectState<Vec<spec::Choice>>>>,
    subscriptions: HashMap<String, Subscription>,
}

impl EventEmitter<A2uiEvent> for A2uiSurface {}

impl A2uiSurface {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            document: Rc::default(),
            mode: Mode::Live,
            touch: false,
            locals: HashMap::new(),
            inputs: HashMap::new(),
            selects: HashMap::new(),
            subscriptions: HashMap::new(),
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            cx.notify();
        }
    }

    pub fn set_touch(&mut self, touch: bool, cx: &mut Context<Self>) {
        if self.touch != touch {
            self.touch = touch;
            cx.notify();
        }
    }

    fn text(&self, style: TextStyle) -> TextStyle {
        match (self.touch, style) {
            (true, TextStyle::Body) => TextStyle::TouchBody,
            (true, TextStyle::Small | TextStyle::Mono) => TextStyle::TouchSmall,
            (_, style) => style,
        }
    }

    pub fn apply(&mut self, message: Message, cx: &mut Context<Self>) {
        for id in Rc::make_mut(&mut self.document).apply(message) {
            let scoped = format!("{id}@");
            let stale = |key: &String| *key == id || key.starts_with(&scoped);
            self.locals.retain(|key, _| !stale(key));
            self.inputs.retain(|key, _| !stale(key));
            self.selects.retain(|key, _| !stale(key));
            self.subscriptions.retain(|key, _| !stale(key));
        }
        cx.notify();
    }

    fn write(&mut self, slot: &Slot, value: Value, cx: &mut Context<Self>) {
        let changed = match slot {
            Slot::Model(pointer) => Rc::make_mut(&mut self.document).set(pointer, value),
            Slot::Local(key) => {
                self.locals.get(key) != Some(&value) && {
                    self.locals.insert(key.clone(), value);
                    true
                }
            }
        };
        if changed {
            cx.notify();
        }
    }

    fn held(&self, slot: &Slot) -> Option<&Value> {
        match slot {
            Slot::Model(pointer) => self.document.value(pointer),
            Slot::Local(key) => self.locals.get(key),
        }
    }

    fn words(&self, words: &Words, slot: &Slot) -> SharedString {
        match (self.held(slot), words) {
            (Some(value), _) => shown(value),
            (None, Dynamic::Literal(scalar)) => scalar_shown(scalar),
            (None, Dynamic::Path { .. }) => SharedString::default(),
        }
    }

    fn number(&self, number: &Dynamic<f64>, slot: &Slot) -> f64 {
        match (self.held(slot).and_then(Value::as_f64), number) {
            (Some(value), _) => value,
            (None, Dynamic::Literal(value)) => *value,
            (None, Dynamic::Path { .. }) => 0.0,
        }
    }

    fn read(&self, words: &Words, base: &str) -> SharedString {
        match words {
            Dynamic::Path { path } => self
                .document
                .value(&absolute(base, path))
                .map(shown)
                .unwrap_or_default(),
            Dynamic::Literal(scalar) => scalar_shown(scalar),
        }
    }

    fn resolve(&self, value: &Value, base: &str) -> Value {
        match value {
            Value::Object(fields) => match fields.get("@path") {
                Some(Value::String(path)) if fields.len() == 1 => self
                    .document
                    .value(&absolute(base, path))
                    .cloned()
                    .unwrap_or(Value::Null),
                _ => Value::Object(
                    fields
                        .iter()
                        .map(|(name, value)| (name.clone(), self.resolve(value, base)))
                        .collect(),
                ),
            },
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.resolve(item, base)).collect())
            }
            other => other.clone(),
        }
    }

    fn fire(&mut self, event: &spec::EventSpec, source: &str, base: &str, cx: &mut Context<Self>) {
        let listed = event
            .order
            .iter()
            .filter_map(|name| event.context.get_key_value(name));
        let rest = event
            .context
            .iter()
            .filter(|(name, _)| !event.order.contains(name));
        let context = listed
            .chain(rest)
            .map(|(name, value)| (name.clone(), self.resolve(value, base)))
            .collect();
        let user_message = event
            .user_message
            .as_ref()
            .map(|words| self.read(words, base).to_string());
        cx.emit(A2uiEvent::Action(Action {
            name: event.name.clone(),
            surface_id: self.id.to_string(),
            source_component_id: source.to_owned(),
            context,
            user_message,
        }));
    }

    fn input(
        &mut self,
        key: &str,
        slot: &Slot,
        placeholder: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(state) = self.inputs.get(key) {
            return state.clone();
        }
        let state = cx.new(|cx| {
            let state = InputState::new(window, cx);
            match placeholder {
                Some(placeholder) => state.placeholder(placeholder.to_owned()),
                None => state,
            }
        });
        let target = slot.clone();
        let subscription = cx.subscribe_in(
            &state,
            window,
            move |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.write(&target, Value::String(value), cx);
                }
            },
        );
        self.subscriptions.insert(key.to_owned(), subscription);
        self.inputs.insert(key.to_owned(), state.clone());
        state
    }

    fn select(
        &mut self,
        key: &str,
        slot: &Slot,
        options: &[spec::Choice],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<Vec<spec::Choice>>> {
        if let Some(state) = self.selects.get(key) {
            return state.clone();
        }
        let state = cx.new(|cx| SelectState::new(options.to_vec(), None, window, cx));
        let target = slot.clone();
        let subscription = cx.subscribe_in(
            &state,
            window,
            move |this, _, event: &SelectEvent<Vec<spec::Choice>>, _, cx| {
                let SelectEvent::Confirm(picked) = event;
                let value = picked.clone().map_or(Value::Null, Value::String);
                this.write(&target, value, cx);
            },
        );
        self.subscriptions.insert(key.to_owned(), subscription);
        self.selects.insert(key.to_owned(), state.clone());
        state
    }

    fn children(
        &mut self,
        document: &Document,
        children: &spec::Children,
        place: Place<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let place = place.deeper();
        match children {
            spec::Children::Ids(ids) => ids
                .iter()
                .map(|id| self.build(document, id, place, window, cx))
                .collect(),
            spec::Children::Template { component_id, path } => {
                let list = absolute(place.base, path);
                let count = document
                    .value(&list)
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                (0..count)
                    .map(|index| {
                        let item = format!("{list}/{index}");
                        self.build(document, component_id, place.at(&item), window, cx)
                    })
                    .collect()
            }
        }
    }

    fn build(
        &mut self,
        document: &Document,
        id: &str,
        place: Place<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(component) = document.component(id).filter(|_| place.depth < MAX_DEPTH) else {
            return div().into_any_element();
        };
        let base = place.base;
        let held = self.mode != Mode::Live;
        let key = if base.is_empty() {
            id.to_owned()
        } else {
            format!("{id}@{base}")
        };
        let element_id = ElementId::from(SharedString::from(key.clone()));
        match &component.kind {
            Kind::Column(flex) => {
                let children = self.children(document, &flex.children, place, window, cx);
                let gap = if place.dense {
                    Space::GAP_RELATED
                } else {
                    Space::S12
                };
                arrange(v_flex().gap(gap), flex, Align::Stretch, children)
            }
            Kind::Row(flex) => {
                let children = self.children(document, &flex.children, place, window, cx);
                arrange(
                    h_flex().flex_wrap().gap(Space::GAP_RELATED),
                    flex,
                    Align::Center,
                    children,
                )
            }
            Kind::Grid(grid) => {
                let children = self.children(document, &grid.children, place, window, cx);
                let columns = if self.touch {
                    grid.columns.min(TOUCH_COLUMNS)
                } else {
                    grid.columns
                };
                div()
                    .grid()
                    .grid_cols(columns.max(1))
                    .gap(Space::GAP_RELATED)
                    .children(children)
                    .into_any_element()
            }
            Kind::Card(card) => {
                let inside = Place {
                    dense: true,
                    ..place.deeper()
                };
                let child = self.build(document, &card.child, inside, window, cx);
                Card::new(element_id, CardLook::Activity)
                    .child(child)
                    .into_any_element()
            }
            Kind::Text(text) => {
                let content = self.read(&text.text, base);
                let label = match text.variant {
                    spec::TextVariant::Body => Label::new(self.text(TextStyle::Body), content),
                    spec::TextVariant::Caption => {
                        Label::new(self.text(TextStyle::Small), content).muted()
                    }
                    spec::TextVariant::Heading if self.touch => {
                        Label::new(TextStyle::TouchBody, content).font_weight(FontWeight::SEMIBOLD)
                    }
                    spec::TextVariant::Heading => Label::new(TextStyle::Heading, content),
                    spec::TextVariant::Title => Label::new(TextStyle::Title, content),
                    spec::TextVariant::Mono => {
                        Label::new(self.text(TextStyle::Mono), content).mono()
                    }
                };
                match text.tone {
                    Some(tone) => label.tone(Tone::from(tone)).into_any_element(),
                    None => label.into_any_element(),
                }
            }
            Kind::Stat(stat) => v_flex()
                .gap(Space::GAP_TIGHT)
                .child(
                    Label::new(self.text(TextStyle::Small), self.read(&stat.label, base)).muted(),
                )
                .child(
                    div()
                        .font_features(tabular())
                        .child(Label::new(TextStyle::Title, self.read(&stat.value, base))),
                )
                .into_any_element(),
            Kind::Badge(badge) => h_flex()
                .child(Badge::new(self.read(&badge.label, base)).tone(Tone::from(badge.tone)))
                .into_any_element(),
            Kind::Button(button) => {
                let pressed = {
                    let event = button.action.event.clone();
                    let source = id.to_owned();
                    let base = base.to_owned();
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.fire(&event, &source, &base, cx);
                    })
                };
                let control = Button::new(element_id)
                    .label(self.read(&button.label, base))
                    .disabled(held);
                let control = if self.touch {
                    control.with_size(Size::Touch).w_full()
                } else {
                    control
                };
                let control = match button.variant {
                    spec::ButtonVariant::Default => control,
                    spec::ButtonVariant::Primary => control.primary(),
                    spec::ButtonVariant::Ghost => control.ghost(),
                };
                let chosen = matches!(
                    &self.mode,
                    Mode::Held { pressed: Some(name) } if *name == button.action.event.name
                );
                let control = if chosen {
                    control.icon(IconName::Check)
                } else {
                    control
                };
                let control = control.on_click(pressed);
                if self.touch {
                    div()
                        .flex_1()
                        .min_w(rems_from_px(TOUCH_BUTTON_WIDTH))
                        .child(control)
                        .into_any_element()
                } else {
                    h_flex().child(control).into_any_element()
                }
            }
            Kind::TextField(field) => {
                let slot = Slot::of(&field.value, &key, base);
                let wanted = self.words(&field.value, &slot);
                let state = self.input(&key, &slot, field.placeholder.as_deref(), window, cx);
                if state.read(cx).value() != wanted {
                    state.update(cx, |state, cx| state.set_value(wanted, window, cx));
                }
                let label = field.label.as_ref().map(|label| self.read(label, base));
                let control = Input::new(&state).disabled(held);
                if self.touch {
                    let control = control.with_size(Size::TouchLarge);
                    labelled(label, control.into_any_element(), true)
                } else {
                    labelled(label, control.into_any_element(), false)
                }
            }
            Kind::Select(select) => {
                let slot = Slot::of(&select.value, &key, base);
                let wanted = self.words(&select.value, &slot).to_string();
                let state = self.select(&key, &slot, &select.options, window, cx);
                let picked = select
                    .options
                    .iter()
                    .position(|option| *option.key() == wanted);
                if state.read(cx).selected_value() != picked.map(|_| &wanted) {
                    state.update(cx, |state, cx| {
                        state.set_selected_index(picked.map(IndexPath::new), window, cx);
                    });
                }
                let control = Select::new(&state).disabled(held).w_full();
                let control = if self.touch {
                    control.with_size(Size::TouchLarge)
                } else {
                    control
                };
                let control = match &select.placeholder {
                    Some(placeholder) => control.placeholder(placeholder.clone()),
                    None => control,
                };
                let label = select.label.as_ref().map(|label| self.read(label, base));
                labelled(label, control.into_any_element(), self.touch)
            }
            Kind::Segmented(segmented) => {
                let slot = Slot::of(&segmented.value, &key, base);
                let wanted = self.words(&segmented.value, &slot);
                let selected = segmented
                    .options
                    .iter()
                    .position(|option| option.key().as_str() == wanted.as_ref())
                    .unwrap_or(0);
                let picked = {
                    let keys: Vec<String> = segmented
                        .options
                        .iter()
                        .map(|option| option.key().clone())
                        .collect();
                    cx.listener(move |this, index: &usize, _, cx| {
                        if let Some(key) = keys.get(*index) {
                            this.write(&slot, Value::String(key.clone()), cx);
                        }
                    })
                };
                let labels = segmented
                    .options
                    .iter()
                    .map(|option| SharedString::from(option.label().to_owned()));
                let control = SegmentedControl::new(element_id, labels)
                    .selected(selected)
                    .disabled(held)
                    .on_select(move |index, window, cx| picked(&index, window, cx));
                if self.touch {
                    control
                        .with_size(Size::Touch)
                        .full_width()
                        .into_any_element()
                } else {
                    h_flex().child(control).into_any_element()
                }
            }
            Kind::Slider(slider) => {
                let slot = Slot::of(&slider.value, &key, base);
                let value = self.number(&slider.value, &slot);
                let moved = cx.listener(move |this, value: &f32, _, cx| {
                    this.write(&slot, rounded(f64::from(*value)), cx);
                });
                let label = slider.label.as_ref().map(|label| self.read(label, base));
                let control = Slider::new(
                    element_id,
                    label.clone().unwrap_or_default(),
                    value as f32,
                    slider.min as f32..=slider.max as f32,
                    move |value, window, cx| moved(&value, window, cx),
                );
                let control = match slider.step {
                    Some(step) => control.step(step as f32),
                    None => control,
                }
                .disabled(held);
                let control = if self.touch || CoarsePointer::get(cx) {
                    div().w_full().child(control).into_any_element()
                } else {
                    let shown = control.shown();
                    h_flex()
                        .w_full()
                        .gap(Space::GAP_RELATED)
                        .child(div().flex_1().min_w_0().child(control))
                        .child(
                            div()
                                .flex_none()
                                .font_features(tabular())
                                .child(Label::new(TextStyle::Small, shown).muted()),
                        )
                        .into_any_element()
                };
                labelled(label, control, self.touch)
            }
            Kind::Progress(progress) => {
                let slot = Slot::of(&progress.value, &key, base);
                let value = self.number(&progress.value, &slot).clamp(0.0, 1.0);
                let bar = Progress::new(element_id, value as f32);
                match &progress.label {
                    Some(label) => {
                        let label = self.read(label, base);
                        let style = self.text(TextStyle::Small);
                        let percent = format!("{:.0}%", value * 100.0);
                        v_flex()
                            .gap(Space::GAP_TIGHT)
                            .child(
                                h_flex()
                                    .justify_between()
                                    .gap(Space::GAP_RELATED)
                                    .child(Label::new(style, label.clone()).muted())
                                    .child(
                                        div()
                                            .flex_none()
                                            .font_features(tabular())
                                            .child(Label::new(style, percent).muted()),
                                    ),
                            )
                            .child(bar.label(label))
                            .into_any_element()
                    }
                    None => bar.into_any_element(),
                }
            }
            Kind::Table(table) => {
                let rows = absolute(base, &table.rows);
                let count = document
                    .value(&rows)
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let cell = |index: usize, column: &spec::TableColumn| {
                    document.value(&absolute(&format!("{rows}/{index}"), &column.path))
                };
                let numeric: Vec<bool> = table
                    .columns
                    .iter()
                    .map(|column| {
                        count > 0
                            && (0..count)
                                .all(|index| cell(index, column).is_some_and(Value::is_number))
                    })
                    .collect();
                let small = self.text(TextStyle::Small);
                let body = self.text(TextStyle::Body);
                let header = table
                    .columns
                    .iter()
                    .map(|column| Label::new(small, column.title.clone()).muted());
                let mut lines = v_flex().child(line(header, &numeric));
                for index in 0..count {
                    let cells = table.columns.iter().map(|column| {
                        Label::new(body, cell(index, column).map(shown).unwrap_or_default())
                    });
                    lines = lines
                        .child(Separator::horizontal())
                        .child(line(cells, &numeric));
                }
                lines.into_any_element()
            }
            Kind::Chart(chart) => {
                let rows = absolute(base, &chart.rows);
                let items = document
                    .value(&rows)
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                let labels = items
                    .iter()
                    .map(|item| item.get(&chart.x).map(shown).unwrap_or_default());
                let series = chart.series.iter().map(|one| {
                    let values = items.iter().map(|item| {
                        item.get(one.path())
                            .and_then(Value::as_f64)
                            .unwrap_or(f64::NAN)
                    });
                    Series::new(one.title().to_owned(), values)
                });
                let kind = match chart.kind {
                    spec::ChartKind::Bar => ChartKind::Bar,
                    spec::ChartKind::Line => ChartKind::Line,
                };
                Chart::new(element_id, kind, labels, series)
                    .touch(self.touch)
                    .into_any_element()
            }
        }
    }
}

impl Render for A2uiSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let document = Rc::clone(&self.document);
        let problems = document
            .problems()
            .into_iter()
            .map(|problem| Field::new().error(problem));
        let place = Place {
            base: "",
            depth: 0,
            dense: false,
        };
        let root = self.build(&document, document.root(), place, window, cx);
        v_flex()
            .w_full()
            .gap(Space::GAP_RELATED)
            .type_style(self.text(TextStyle::Body), cx)
            .font_family(cx.theme().font_family.clone())
            .text_color(cx.theme().foreground)
            .children(problems)
            .child(root)
    }
}

fn arrange(
    container: ala::Div,
    flex: &spec::Flex,
    align: Align,
    children: Vec<AnyElement>,
) -> AnyElement {
    let container = container.min_w_0();
    let container = match flex.justify {
        Justify::Start => container.justify_start(),
        Justify::Center => container.justify_center(),
        Justify::End => container.justify_end(),
        Justify::SpaceBetween => container.justify_between(),
    };
    let container = match flex.align.unwrap_or(align) {
        Align::Start => container.items_start(),
        Align::Center => container.items_center(),
        Align::End => container.items_end(),
        Align::Stretch => container.items_stretch(),
    };
    container.children(children).into_any_element()
}

fn labelled(label: Option<SharedString>, control: AnyElement, touch: bool) -> AnyElement {
    let field = match label {
        Some(label) => Field::new().label(label),
        None => Field::new(),
    };
    if touch {
        field
            .with_size(Size::TouchLarge)
            .w_full()
            .child(control)
            .into_any_element()
    } else {
        field
            .w_full()
            .max_w(rems_from_px(FIELD_WIDTH))
            .items_stretch()
            .child(control)
            .into_any_element()
    }
}

fn line(cells: impl Iterator<Item = Label>, numeric: &[bool]) -> ala::Div {
    h_flex()
        .gap(Space::GAP_GROUP)
        .py(Space::S4)
        .children(cells.zip(numeric).map(|(cell, numeric)| {
            let cell = h_flex().flex_1().min_w_0().child(cell);
            if *numeric {
                cell.justify_end().font_features(tabular())
            } else {
                cell
            }
        }))
}

fn tabular() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

fn shown(value: &Value) -> SharedString {
    match value {
        Value::String(text) => SharedString::from(text.clone()),
        Value::Number(number) => SharedString::from(number.to_string()),
        Value::Bool(flag) => SharedString::from(flag.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => SharedString::default(),
    }
}

fn scalar_shown(scalar: &Scalar) -> SharedString {
    match scalar {
        Scalar::Text(text) => SharedString::from(text.clone()),
        Scalar::Number(number) => shown(&rounded(*number)),
        Scalar::Flag(flag) => SharedString::from(flag.to_string()),
    }
}

fn rounded(value: f64) -> Value {
    let value = (value * 1000.0).round() / 1000.0;
    if value.fract() == 0.0 && value.abs() < 9e15 {
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
}

use std::{cell::RefCell, rc::Rc, sync::Arc};

use zz_client::{
    completion::PaneKindAvailability,
    gui_actions::{GuiOffer, GuiOption, PANE_GAPS, THEME, gui_action},
    place::ActionKind,
    recents::RecentKey,
};
use zz_gpui::{
    App, AppContext as _, Context, Entity, KeyDownEvent, SharedString, TestAppContext,
    VisualTestContext, Window,
};
use zz_protocol::{CommandInvocation, InputMessage, MuxSnapshot, PaneId, SessionId, WindowId};

use crate::{
    IconName, Root,
    command::{
        CommandPaletteEvent, CommandPaletteView, GuiEvent, GuiLevel, PaletteActionLevel,
        PaletteBackend, PaletteHostId, PaletteSettings, PaletteStatus, PaletteTarget, PaletteTree,
        PaletteTreeHost, PaletteTreePane, PaletteTreeSession, PaletteTreeWindow,
        palette_model::PaletteAction, record_recent,
    },
};

struct Backend {
    gui: Vec<GuiOffer>,
}

impl PaletteBackend for Backend {
    fn snapshot(&self, _: &App) -> Arc<MuxSnapshot> {
        Arc::default()
    }

    fn host_snapshot<'a>(&self, _: PaletteHostId, _: &'a App) -> Option<&'a MuxSnapshot> {
        None
    }

    fn tree(&self, _: &App) -> PaletteTree {
        PaletteTree {
            attached: PaletteHostId(0),
            hosts: vec![PaletteTreeHost {
                id: PaletteHostId(0),
                name: "local".to_owned(),
                detail: String::new(),
                right: String::new(),
                status: PaletteStatus::Online,
                sessions: vec![PaletteTreeSession {
                    id: SessionId(1),
                    name: "dev".to_owned(),
                    tmux_name: "dev".to_owned(),
                    active: true,
                    windows: vec![PaletteTreeWindow {
                        id: WindowId(1),
                        index: 1,
                        name: "editor".to_owned(),
                        active: true,
                        active_pane: PaneId(10),
                        panes: vec![PaletteTreePane {
                            id: PaneId(10),
                            index: 0,
                            label: "nvim".to_owned(),
                            detail: "Terminal".into(),
                            program: None,
                            icon: IconName::SquareTerminal,
                            running: false,
                            needs_you: false,
                            terminal: true,
                            cwd: String::new(),
                        }],
                    }],
                }],
            }],
        }
    }

    fn settings(&self, _: &App) -> PaletteSettings {
        PaletteSettings::default()
    }

    fn availability(&self, _: &App) -> PaneKindAvailability {
        PaneKindAvailability::default()
    }

    fn command_shortcut(&self, _: &str, _: &App) -> Option<SharedString> {
        None
    }

    fn mono_font(&self, _: &App) -> SharedString {
        "Lilex".into()
    }

    fn send_input(&self, _: InputMessage, _: &mut App) {}

    fn send_key(&self, _: PaneId, _: &KeyDownEvent, _: &mut App) {}

    fn active_pane(&self, _: &App) -> Option<PaneId> {
        Some(PaneId(10))
    }

    fn execute(&self, _: PaletteHostId, _: CommandInvocation, _: &mut App) {}

    fn connect_host(&self, _: PaletteHostId, _: &mut App) {}

    fn activate(&self, _: PaletteHostId, _: PaletteTarget, _: &mut App) -> bool {
        true
    }

    fn verbs(&self, _: &App) -> Vec<zz_client::verbs::Verb> {
        zz_client::verbs::verbs(
            PaneKindAvailability::default(),
            &[PANE_GAPS, THEME],
            false,
            "",
        )
    }

    fn action_level(&self, kind: ActionKind, _: &mut App) -> Option<Box<dyn PaletteActionLevel>> {
        match kind {
            ActionKind::Gui => Some(Box::new(GuiLevel::new(self.gui.clone()))),
            ActionKind::Command | ActionKind::Files => None,
        }
    }
}

fn offers() -> Vec<GuiOffer> {
    let themes = [("default", "Default"), ("macos", "macOS"), ("yaru", "Yaru")]
        .map(|(value, title)| GuiOption {
            value: value.to_owned(),
            title: title.to_owned(),
        })
        .to_vec();
    vec![
        GuiOffer::toggle(PANE_GAPS, false).unwrap(),
        GuiOffer::listed(THEME, "default", themes).unwrap(),
    ]
}

type Heard = Rc<RefCell<Vec<CommandPaletteEvent>>>;

fn mount(cx: &mut TestAppContext) -> (Entity<CommandPaletteView>, Heard, &mut VisualTestContext) {
    cx.update(crate::init);
    let slot = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let backend: Rc<dyn PaletteBackend> = Rc::new(Backend { gui: offers() });
        let palette = cx.new(|cx: &mut Context<CommandPaletteView>| {
            CommandPaletteView::new_unified(backend, None, window, cx)
        });
        palette.read(cx).focus(cx).focus(window, cx);
        captured.replace(Some(palette.clone()));
        Root::new(palette, window, cx)
    });
    let palette = slot.borrow_mut().take().expect("palette captured");
    let heard = Heard::default();
    let events = Rc::clone(&heard);
    cx.update(|_: &mut Window, cx: &mut App| {
        cx.subscribe(&palette, move |_, event: &CommandPaletteEvent, _| {
            events.borrow_mut().push(match event {
                CommandPaletteEvent::ReturnToDefault => CommandPaletteEvent::ReturnToDefault,
                CommandPaletteEvent::Gui(event) => CommandPaletteEvent::Gui(event.clone()),
            });
        })
        .detach();
    });
    (palette, heard, cx)
}

fn gui(heard: &Heard) -> Vec<GuiEvent> {
    heard
        .borrow()
        .iter()
        .filter_map(|event| match event {
            CommandPaletteEvent::Gui(event) => Some(event.clone()),
            CommandPaletteEvent::ReturnToDefault => None,
        })
        .collect()
}

fn finished(palette: &Entity<CommandPaletteView>, cx: &mut VisualTestContext) -> bool {
    palette.read_with(cx, |palette, _| palette.is_finished())
}

#[zz_gpui::test]
fn a_theme_previews_while_highlighted_and_applies_on_enter(cx: &mut TestAppContext) {
    let (palette, heard, cx) = mount(cx);
    let theme = gui_action(THEME).unwrap();
    cx.simulate_input("theme");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(gui(&heard).is_empty(), "the list opens on the theme in use");
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let preview = |value: &str| GuiEvent::Preview {
        action: theme,
        value: value.to_owned(),
    };
    assert_eq!(gui(&heard), [preview("macos")]);
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    assert_eq!(gui(&heard).last(), Some(&GuiEvent::Revert));
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    assert_eq!(
        gui(&heard)[2..],
        [
            preview("macos"),
            preview("yaru"),
            GuiEvent::Apply {
                action: theme,
                value: "yaru".to_owned()
            }
        ]
    );
    assert!(finished(&palette, cx));
    let recents = cx.update(|_, cx| crate::command::palette_recents(cx));
    assert!(
        recents
            .top(
                zz_client::recents::RecentKind::Gui,
                3,
                zz_client::recents::now_secs()
            )
            .iter()
            .any(
                |recent| recent.key == RecentKey::Gui("interface-theme=yaru".to_owned())
                    && recent.label == "Theme › Yaru"
            )
    );
}

#[zz_gpui::test]
fn enter_flips_an_on_off_action(cx: &mut TestAppContext) {
    let (palette, heard, cx) = mount(cx);
    cx.simulate_input("gaps");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        gui(&heard),
        [GuiEvent::Apply {
            action: gui_action(PANE_GAPS).unwrap(),
            value: "true".to_owned()
        }]
    );
    assert!(finished(&palette, cx));
}

#[zz_gpui::test]
fn backspace_leaves_the_values_and_puts_the_theme_back(cx: &mut TestAppContext) {
    let (palette, heard, cx) = mount(cx);
    cx.simulate_input("theme");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter down");
    cx.run_until_parked();
    assert!(matches!(gui(&heard)[..], [GuiEvent::Preview { .. }]));
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(gui(&heard).last(), Some(&GuiEvent::Revert));
    assert!(palette.read_with(cx, |palette, _| {
        palette.unified.as_ref().unwrap().draft.is_none()
    }));
    assert!(!finished(&palette, cx));
}

#[zz_gpui::test]
fn a_recent_action_applies_its_value_again(cx: &mut TestAppContext) {
    let (palette, heard, cx) = mount(cx);
    cx.update(|window, cx| {
        record_recent(
            RecentKey::Gui("pane-gaps=true".to_owned()),
            "Pane gaps › On",
            cx,
        );
        palette.update(cx, |palette, cx| palette.refresh(window, cx));
    });
    let index = palette.read_with(cx, |palette, _| {
        palette
            .unified
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|entry| {
                matches!(
                    &entry.action,
                    Some(PaletteAction::Recent(RecentKey::Gui(_)))
                )
            })
    });
    palette.update(cx, |palette, _| palette.selected = index);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        gui(&heard),
        [GuiEvent::Apply {
            action: gui_action(PANE_GAPS).unwrap(),
            value: "true".to_owned()
        }]
    );
    assert!(finished(&palette, cx));
}

#[test]
fn the_theme_offer_names_themes_as_the_theme_picker_does() {
    use crate::interface_theme::{ThemeOverrides, default_theme};
    let copy = default_theme().clone();
    let overrides = ThemeOverrides::default();
    let picks = crate::settings::appearance::ThemePicks {
        themes: vec![("default".into(), default_theme()), ("mine".into(), &copy)],
        selected: 1,
        overrides: &overrides,
    };
    let offer = crate::command::theme_offer(&picks).unwrap();
    assert_eq!(offer.current, "mine");
    let titles: Vec<_> = offer
        .options
        .iter()
        .map(|option| option.title.clone())
        .collect();
    assert_eq!(
        titles,
        [
            default_theme().name.clone(),
            format!("{} (mine)", default_theme().name)
        ]
    );
}

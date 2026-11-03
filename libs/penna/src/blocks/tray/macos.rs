use ala::{Bounds, DisplayId, point, px, size};
use async_channel::Sender;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, Sel};
use objc2::{
    AnyThread as _, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, sel,
};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventMask,
    NSEventModifierFlags, NSEventType, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSData, NSNumber, NSSize, NSString, ns_string};

use super::{Menu, MenuEntry, TrayAnchor, TrayEvent, TrayIcon, TrayOptions, send};

const IMAGE_POINTS: f64 = 18.0;

pub(super) struct StatusItem {
    item: Retained<NSStatusItem>,
    _target: Retained<TrayTarget>,
}

impl StatusItem {
    pub(super) fn set_badge(&self, badge: &str) {
        if let Some(mtm) = MainThreadMarker::new()
            && let Some(button) = self.item.button(mtm)
        {
            button.setTitle(&NSString::from_str(badge));
        }
    }

    pub(super) fn set_look(&self, tooltip: &str, icon: &TrayIcon) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if let Some(button) = self.item.button(mtm) {
            if let Some(image) = image(icon) {
                button.setImage(Some(&image));
            }
            button.setToolTip(Some(&NSString::from_str(tooltip)));
        }
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

pub(super) fn spawn(
    options: TrayOptions,
    menu: Menu,
    events: Sender<TrayEvent>,
) -> Option<StatusItem> {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "penna::tray", "could not create the status item outside the main thread");
        return None;
    };
    let TrayOptions {
        tooltip,
        icon,
        menu_on_click,
        ..
    } = options;
    let image = image(&icon)?;
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    let Some(button) = item.button(mtm) else {
        log::warn!(target: "penna::tray", "AppKit gave the status item no button");
        NSStatusBar::systemStatusBar().removeStatusItem(&item);
        return None;
    };
    button.setImage(Some(&image));
    button.setToolTip(Some(&NSString::from_str(&tooltip)));

    send(&events, TrayEvent::Available(true));
    let target = TrayTarget::new(events, item.clone(), menu, menu_on_click, mtm);
    let any_target: &AnyObject = &target;
    #[allow(
        unsafe_code,
        reason = "target/action wiring has no safe objc2 binding: AppKit cannot type-check either half"
    )]
    unsafe {
        button.setTarget(Some(any_target));
        button.setAction(Some(sel!(trayClicked:)));
    }
    button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp);

    Some(StatusItem {
        item,
        _target: target,
    })
}

fn image(icon: &TrayIcon) -> Option<Retained<NSImage>> {
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&icon.png))?;
    image.setSize(NSSize::new(IMAGE_POINTS, IMAGE_POINTS));
    image.setTemplate(icon.template);
    Some(image)
}

struct TrayIvars {
    menu: Menu,
    menu_on_click: bool,
    actions: std::cell::RefCell<Vec<Option<String>>>,
    events: Sender<TrayEvent>,
    item: Retained<NSStatusItem>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TrayIvars]
    struct TrayTarget;

    impl TrayTarget {
        #[unsafe(method(trayClicked:))]
        fn tray_clicked(&self, _sender: Option<&AnyObject>) {
            if self.ivars().menu_on_click || self.click_was_secondary() {
                self.show_menu();
            } else {
                send(&self.ivars().events, TrayEvent::Activate(self.anchor()));
            }
        }

        #[unsafe(method(trayAction:))]
        fn tray_action(&self, sender: &NSMenuItem) {
            if let Ok(index) = usize::try_from(sender.tag())
                && let Some(Some(action)) = self.ivars().actions.borrow().get(index)
            {
                send(&self.ivars().events, TrayEvent::Action(action.clone()));
            }
        }
    }

    unsafe impl NSObjectProtocol for TrayTarget {}
);

impl TrayTarget {
    #[allow(
        unsafe_code,
        reason = "`init` on the superclass has no safe objc2 binding"
    )]
    fn new(
        events: Sender<TrayEvent>,
        item: Retained<NSStatusItem>,
        menu: Menu,
        menu_on_click: bool,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TrayIvars {
            events,
            item,
            menu,
            menu_on_click,
            actions: std::cell::RefCell::new(Vec::new()),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn anchor(&self) -> Option<TrayAnchor> {
        let window = self.ivars().item.button(self.mtm())?.window()?;
        let screen = window.screen()?;
        let number = screen
            .deviceDescription()
            .objectForKey(ns_string!("NSScreenNumber"))?
            .downcast::<NSNumber>()
            .ok()?;
        let (frame, area) = (window.frame(), screen.frame());
        let top = area.origin.y + area.size.height - frame.origin.y - frame.size.height;
        Some(TrayAnchor {
            display: DisplayId::from(u64::from(number.unsignedIntValue())),
            bounds: Bounds::new(
                point(px((frame.origin.x - area.origin.x) as f32), px(top as f32)),
                size(px(frame.size.width as f32), px(frame.size.height as f32)),
            ),
        })
    }

    fn click_was_secondary(&self) -> bool {
        NSApplication::sharedApplication(self.mtm())
            .currentEvent()
            .is_some_and(|event| {
                event.r#type() == NSEventType::RightMouseUp
                    || event
                        .modifierFlags()
                        .contains(NSEventModifierFlags::Control)
            })
    }

    #[allow(
        unsafe_code,
        reason = "`performClick:` has no safe objc2 binding: AppKit cannot know what the click will run"
    )]
    fn show_menu(&self) {
        let mtm = self.mtm();
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!(""));
        menu.setAutoenablesItems(false);
        let mut actions = Vec::new();
        for entry in self.ivars().menu.entries() {
            match entry {
                MenuEntry::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
                MenuEntry::Item {
                    label,
                    action,
                    checked,
                } => {
                    let row = self.menu_item(&NSString::from_str(&label), sel!(trayAction:), mtm);
                    row.setTag(actions.len() as isize);
                    row.setEnabled(action.is_some());
                    if let Some(checked) = checked {
                        row.setState(if checked {
                            NSControlStateValueOn
                        } else {
                            NSControlStateValueOff
                        });
                    }
                    menu.addItem(&row);
                    actions.push(action);
                }
            }
        }
        *self.ivars().actions.borrow_mut() = actions;

        let item = &self.ivars().item;
        item.setMenu(Some(&menu));
        if let Some(button) = item.button(mtm) {
            unsafe { button.performClick(None) };
        }
        item.setMenu(None);
    }

    #[allow(
        unsafe_code,
        reason = "target/action wiring has no safe objc2 binding: AppKit cannot type-check either half"
    )]
    fn menu_item(
        &self,
        title: &NSString,
        action: Sel,
        mtm: MainThreadMarker,
    ) -> Retained<NSMenuItem> {
        let target: &AnyObject = self;
        unsafe {
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                title,
                Some(action),
                ns_string!(""),
            );
            item.setTarget(Some(target));
            item
        }
    }
}

use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::marker::PhantomData;

type PaneId = u64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClipboardProducer {
    Server,
    Application,
}

#[derive(Clone, Copy)]
enum ClipboardTarget {
    Clipboard,
    Primary,
}

enum MuxOptionKey {
    SetClipboard,
}

struct OptionValue {
    value: String,
}

struct Core {
    option: Option<OptionValue>,
}

impl Core {
    fn mux_options(&self) -> &Self {
        self
    }
    fn get(&self, _: MuxOptionKey) -> Option<&OptionValue> {
        self.option.as_ref()
    }
}

struct Terminal {
    copy_generation: u64,
}

struct Retained(RefCell<Terminal>);

impl Retained {
    fn write(&self) -> RefMut<'_, Terminal> {
        self.0.borrow_mut()
    }
}

#[derive(Clone)]
struct ClipboardItem {
    text: String,
}

impl ClipboardItem {
    fn new_string(text: String) -> Self {
        Self { text }
    }
}

struct Context<T> {
    clipboard: Vec<String>,
    primary: Vec<String>,
    marker: PhantomData<T>,
}

impl<T> Context<T> {
    fn write_to_clipboard(&mut self, item: ClipboardItem) {
        self.clipboard.push(item.text);
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn write_to_primary(&mut self, item: ClipboardItem) {
        self.primary.push(item.text);
    }
}

struct MuxClient {
    core: Core,
    viewports: HashMap<PaneId, Retained>,
}

impl MuxClient {
    include!("clipboard_policy_source.rs");
}

fn main() {
    for (option_index, option) in [
        None,
        Some("external"),
        Some("on"),
        Some("off"),
        Some(""),
        Some("other"),
    ]
    .into_iter()
    .enumerate()
    {
        for (producer_index, producer) in
            [ClipboardProducer::Server, ClipboardProducer::Application]
                .into_iter()
                .enumerate()
        {
            for (target_index, target) in [ClipboardTarget::Clipboard, ClipboardTarget::Primary]
                .into_iter()
                .enumerate()
            {
                for empty in [false, true] {
                    for retained in [false, true] {
                        for initial in [0, u64::MAX] {
                            let mut client = MuxClient {
                                core: Core {
                                    option: option.map(|value| OptionValue {
                                        value: value.to_owned(),
                                    }),
                                },
                                viewports: HashMap::new(),
                            };
                            if retained {
                                client.viewports.insert(
                                    7,
                                    Retained(RefCell::new(Terminal {
                                        copy_generation: initial,
                                    })),
                                );
                            }
                            let mut context = Context {
                                clipboard: Vec::new(),
                                primary: Vec::new(),
                                marker: PhantomData,
                            };
                            client.write_clipboard(
                                7,
                                producer,
                                target,
                                if empty {
                                    String::new()
                                } else {
                                    "native clipboard".to_owned()
                                },
                                &mut context,
                            );
                            let generation = client
                                .viewports
                                .get(&7)
                                .map_or(initial, |value| value.0.borrow().copy_generation);
                            println!(
                                "{option_index} {producer_index} {target_index} {} {} {initial} {} {} {generation}",
                                u8::from(empty),
                                u8::from(retained),
                                context.clipboard.len(),
                                context.primary.len()
                            );
                        }
                    }
                }
            }
        }
    }
}

use std::rc::Rc;

use ala::TestAppContext;

use super::test_support::*;
use super::tests::*;
use super::*;

fn install_kitty_lifecycle_image(
    mux: &mut MuxClient,
    pane: PaneId,
    generation: u64,
    cx: &mut Context<MuxClient>,
) -> Arc<RenderImage> {
    let mut viewport = TerminalViewport::blank_with_appearance(
        20,
        12,
        zz_terminal::SessionStatus::Starting,
        &TerminalAppearance::default(),
    );
    viewport.generation = generation;
    viewport.kitty_placements = Arc::from([zz_terminal::KittyPlacement {
        image_id: 4,
        image_generation: generation,
        layer: zz_terminal::KittyLayer::AboveText,
        viewport_col: 0,
        viewport_row: 0,
        absolute_row: 0,
        cell_offset_x: 0,
        cell_offset_y: 0,
        grid_cols: 2,
        grid_rows: 2,
        pixel_width: 8,
        pixel_height: 8,
        source_rect: None,
    }]);
    mux.apply_terminal_viewport(pane, viewport, cx);
    mux.begin_kitty_image(pane, 4, generation, 8, 8, 256);
    mux.push_kitty_image_chunk(pane, 4, generation, &[127; 256]);
    mux.kitty_images(pane)
        .unwrap()
        .read()
        .image(4, generation)
        .unwrap()
}

#[ala::test]
fn kitty_atlas_image_survives_overlapping_view_handoff(cx: &mut TestAppContext) {
    use crate::terminal::view::TerminalView;
    use std::cell::Cell;

    let (mux, image) = cx.update(|cx| {
        zz_ui::init(cx);
        let mux = test_mux(cx);
        let image = mux.update(cx, |mux, cx| {
            install_kitty_lifecycle_image(mux, PaneId(7), 1, cx)
        });
        (mux, image)
    });
    let (root, cx) = cx.add_window_view(|window, cx| {
        let terminal = cx.new(|cx| {
            TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
        });
        terminal.read(cx).focus().focus(window, cx);
        KittyPaneHandoffRoot { terminal }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&image)));

    let previous = cx.update(|window, cx| {
        let previous = root.update(cx, |root, cx| {
            let terminal = cx.new(|cx| {
                TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
            });
            terminal.read(cx).focus().focus(window, cx);
            let previous = std::mem::replace(&mut root.terminal, terminal);
            let weak = previous.downgrade();
            drop(previous);
            cx.notify();
            weak
        });
        assert!(
            previous.upgrade().is_some(),
            "the previous frame's focused input handler must retain the old view"
        );
        window.draw(cx).clear(cx);
        assert!(window.has_image_atlas_entry(&image));
        previous
    });
    cx.run_until_parked();
    assert!(previous.upgrade().is_none());
    cx.update(|window, _| {
        assert!(
            window.has_image_atlas_entry(&image),
            "releasing the old view evicted the replacement view's image"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&image)));
}

#[ala::test]
fn kitty_atlas_cache_replacement_survives_overlapping_view_handoff(cx: &mut TestAppContext) {
    use crate::terminal::view::TerminalView;
    use std::cell::Cell;

    let (mux, first, old_cache) = cx.update(|cx| {
        zz_ui::init(cx);
        let mux = test_mux(cx);
        let first = mux.update(cx, |mux, cx| {
            install_kitty_lifecycle_image(mux, PaneId(7), 1, cx)
        });
        let cache = mux.read(cx).kitty_images(PaneId(7)).unwrap();
        (mux, first, cache)
    });
    let (root, cx) = cx.add_window_view(|window, cx| {
        let terminal = cx.new(|cx| {
            TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
        });
        terminal.read(cx).focus().focus(window, cx);
        KittyPaneHandoffRoot { terminal }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&first)));

    let (previous, second, new_cache) = cx.update(|window, cx| {
        let previous = root.update(cx, |root, cx| {
            let terminal = cx.new(|cx| {
                TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
            });
            terminal.read(cx).focus().focus(window, cx);
            let previous = std::mem::replace(&mut root.terminal, terminal);
            let weak = previous.downgrade();
            drop(previous);
            cx.notify();
            weak
        });
        assert!(previous.upgrade().is_some());
        assert_eq!(old_cache.read().view_count, 2);
        let second = mux.update(cx, |mux, cx| {
            mux.clear_kitty_images(PaneId(7));
            let image = install_kitty_lifecycle_image(mux, PaneId(7), 2, cx);
            cx.notify();
            image
        });
        let new_cache = mux.read(cx).kitty_images(PaneId(7)).unwrap();
        (previous, second, new_cache)
    });
    cx.run_until_parked();
    cx.update(|_, _| {});
    assert!(previous.upgrade().is_none());
    assert_eq!(old_cache.read().view_count, 0);
    assert_eq!(new_cache.read().view_count, 1);
    cx.update(|window, _| {
        assert!(!window.has_image_atlas_entry(&first));
        assert!(window.has_image_atlas_entry(&second));
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&second)));
}

#[ala::test]
fn kitty_atlas_view_registration_releases_after_window_close(cx: &mut TestAppContext) {
    use crate::terminal::view::TerminalView;
    use std::cell::Cell;

    let (mux, image, cache) = cx.update(|cx| {
        zz_ui::init(cx);
        cx.set_quit_mode(ala::QuitMode::Explicit);
        let mux = test_mux(cx);
        let image = mux.update(cx, |mux, cx| {
            install_kitty_lifecycle_image(mux, PaneId(7), 1, cx)
        });
        let cache = mux.read(cx).kitty_images(PaneId(7)).unwrap();
        (mux, image, cache)
    });
    let (root, cx) = cx.add_window_view(|window, cx| KittyPaneHandoffRoot {
        terminal: cx.new(|cx| {
            TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
        }),
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&image)));
    let retained = root.read_with(cx, |root, _| root.terminal.clone());
    let weak = retained.downgrade();
    drop(root);
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    assert!(weak.upgrade().is_some());
    assert_eq!(cache.read().view_count, 1);
    cx.cx.update(|_| drop(retained));
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
    assert_eq!(cache.read().view_count, 0);
}

#[ala::test]
fn kitty_atlas_entries_follow_pane_and_cache_lifetimes(cx: &mut TestAppContext) {
    use crate::terminal::view::TerminalView;
    use std::cell::Cell;

    let (mux, first, other_image) = cx.update(|cx| {
        zz_ui::init(cx);
        let mux = test_mux(cx);
        let (first, other) = mux.update(cx, |mux, cx| {
            (
                install_kitty_lifecycle_image(mux, PaneId(7), 1, cx),
                install_kitty_lifecycle_image(mux, PaneId(8), 1, cx),
            )
        });
        (mux, first, other)
    });
    let (root, cx) = cx.add_window_view(|window, cx| KittyPaneLifecycleRoot {
        terminal: Some(cx.new(|cx| {
            TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
        })),
        other: Some(cx.new(|cx| {
            TerminalView::new(PaneId(8), mux.clone(), Rc::new(Cell::new(true)), window, cx)
        })),
        shown: true,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| {
        assert!(window.has_image_atlas_entry(&first));
        assert!(window.has_image_atlas_entry(&other_image));
    });

    let first_view = root.update(cx, |root, cx| {
        let view = root.terminal.take().unwrap();
        let weak = view.downgrade();
        drop(view);
        cx.notify();
        weak
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(first_view.upgrade().is_none());
    cx.update(|window, _| {
        assert!(
            !window.has_image_atlas_entry(&first),
            "closed terminal image remains in the atlas"
        );
        assert!(window.has_image_atlas_entry(&other_image));
    });
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.terminal = Some(cx.new(|cx| {
                TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
            }));
            cx.notify();
        });
        window.draw(cx).clear(cx);
    });
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&first)));

    root.update(cx, |root, cx| {
        root.shown = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let second = mux.update(cx, |mux, cx| {
        let image = install_kitty_lifecycle_image(mux, PaneId(7), 2, cx);
        cx.notify();
        image
    });
    cx.run_until_parked();
    cx.update(|window, _| {
        assert!(!window.has_image_atlas_entry(&first));
        assert!(!window.has_image_atlas_entry(&second));
        assert!(window.has_image_atlas_entry(&other_image));
    });
    root.update(cx, |root, cx| {
        root.shown = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&second)));

    let third = mux.update(cx, |mux, cx| {
        mux.clear_kitty_images(PaneId(7));
        let image = install_kitty_lifecycle_image(mux, PaneId(7), 3, cx);
        cx.notify();
        image
    });
    cx.run_until_parked();
    cx.update(|window, _| assert!(!window.has_image_atlas_entry(&second)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&third)));

    root.update(cx, |root, cx| {
        root.shown = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    mux.update(cx, |mux, cx| {
        mux.remove_kitty_images(PaneId(7), &[4]);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, _| assert!(!window.has_image_atlas_entry(&third)));
    root.update(cx, |root, cx| {
        root.shown = true;
        cx.notify();
    });

    for generation in 4..7 {
        let image = mux.update(cx, |mux, cx| {
            let image = install_kitty_lifecycle_image(mux, PaneId(7), generation, cx);
            cx.notify();
            image
        });
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                if root.terminal.is_none() {
                    root.terminal = Some(cx.new(|cx| {
                        TerminalView::new(
                            PaneId(7),
                            mux.clone(),
                            Rc::new(Cell::new(true)),
                            window,
                            cx,
                        )
                    }));
                    cx.notify();
                }
            });
            window.draw(cx).clear(cx);
        });
        cx.update(|window, _| assert!(window.has_image_atlas_entry(&image)));
        let weak = root.update(cx, |root, cx| {
            if generation == 6 {
                mux.update(cx, |mux, cx| {
                    mux.forget_pane(PaneId(7));
                    cx.notify();
                });
            }
            let view = root.terminal.take().unwrap();
            let weak = view.downgrade();
            drop(view);
            cx.notify();
            weak
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
        cx.update(|window, cx| {
            assert!(!window.has_image_atlas_entry(&image));
            assert!(window.has_image_atlas_entry(&other_image));
            if generation < 6 {
                assert!(
                    mux.read(cx)
                        .kitty_images(PaneId(7))
                        .unwrap()
                        .read()
                        .image(4, generation)
                        .is_some()
                );
            } else {
                assert!(mux.read(cx).kitty_images(PaneId(7)).is_none());
            }
        });
    }

    let (unpainted_image, unpainted_view) = cx.update(|window, cx| {
        let image = mux.update(cx, |mux, cx| {
            install_kitty_lifecycle_image(mux, PaneId(7), 7, cx)
        });
        let unpainted = cx.new(|cx| {
            TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
        });
        mux.update(cx, |mux, cx| {
            mux.forget_pane(PaneId(7));
            cx.notify();
        });
        let weak = unpainted.downgrade();
        drop(unpainted);
        (image, weak)
    });
    cx.run_until_parked();
    assert!(unpainted_view.upgrade().is_none());
    cx.update(|window, _| {
        assert!(!window.has_image_atlas_entry(&unpainted_image));
        assert!(window.has_image_atlas_entry(&other_image));
    });

    let final_image = mux.update(cx, |mux, cx| {
        install_kitty_lifecycle_image(mux, PaneId(7), 8, cx)
    });
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.terminal = Some(cx.new(|cx| {
                TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
            }));
            cx.notify();
        });
        window.draw(cx).clear(cx);
    });
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&final_image)));
    let (last_image, released_views) = root.update(cx, |root, cx| {
        let last_image = mux.update(cx, |mux, cx| {
            install_kitty_lifecycle_image(mux, PaneId(7), 9, cx)
        });
        let first = root.terminal.take().unwrap();
        let second = root.other.take().unwrap();
        let weak = [first.downgrade(), second.downgrade()];
        drop((first, second));
        cx.notify();
        (last_image, weak)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(released_views.iter().all(|view| view.upgrade().is_none()));
    cx.update(|window, cx| {
        for image in [&final_image, &last_image, &other_image] {
            assert!(!window.has_image_atlas_entry(image));
        }
        for (pane, generation) in [(PaneId(7), 9), (PaneId(8), 1)] {
            let cache = mux.read(cx).kitty_images(pane).unwrap();
            assert!(cache.read().retired.is_empty());
            assert!(cache.read().image(4, generation).is_some());
        }
    });
}

#[ala::test]
fn kitty_image_chunks_assemble_replace_remove_and_deduplicate(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mux = test_mux(cx);
        let pane = PaneId(7);
        mux.update(cx, |mux, _| {
            mux.begin_kitty_image(pane, 4, 1, 2, 1, 8);
            mux.push_kitty_image_chunk(pane, 4, 1, &[0, 1, 2]);
            assert!(
                mux.kitty_images(pane)
                    .expect("pane cache")
                    .read()
                    .image(4, 1)
                    .is_none()
            );
            mux.push_kitty_image_chunk(pane, 4, 1, &[3, 4, 5, 6, 7]);
        });
        let cache = mux.read(cx).kitty_images(pane).expect("pane cache");
        let first = cache.read().image(4, 1).expect("assembled image");
        assert_eq!(first.as_bytes(0), Some([0, 1, 2, 3, 4, 5, 6, 7].as_slice()));

        mux.update(cx, |mux, _| {
            mux.begin_kitty_image(pane, 4, 1, 2, 1, 8);
            mux.push_kitty_image_chunk(pane, 4, 1, &[9; 8]);
        });
        let unchanged = cache.read().image(4, 1).expect("unchanged image");
        assert!(Arc::ptr_eq(&first, &unchanged));

        mux.update(cx, |mux, _| {
            mux.begin_kitty_image(pane, 4, 2, 1, 1, 4);
            mux.push_kitty_image_chunk(pane, 4, 2, &[8, 9, 10, 11]);
        });
        let replacement = cache.read().image(4, 2).expect("replacement image");
        assert!(!Arc::ptr_eq(&first, &replacement));
        assert_eq!(cache.write().take_retired().len(), 1);

        mux.update(cx, |mux, _| {
            mux.begin_kitty_image(pane, 4, 3, 1, 1, 4);
            mux.push_kitty_image_chunk(pane, 4, 3, &[0; 5]);
        });
        assert!(cache.read().image(4, 3).is_none());
        assert!(
            !mux.read(cx)
                .kitty_image_assemblies
                .contains_key(&(pane, 4, 3))
        );

        mux.update(cx, |mux, _| mux.remove_kitty_images(pane, &[4]));
        assert!(cache.read().image(4, 2).is_none());
    });
}

#[test]
fn pane_image_snapshots_use_daemon_observed_numbers() {
    let mut snapshots = PaneImageSnapshots::default();
    snapshots.insert(
        12,
        Arc::new(Image::from_bytes(ala::ImageFormat::Png, vec![12])),
    );
    snapshots.insert(
        1,
        Arc::new(Image::from_bytes(ala::ImageFormat::Png, vec![1])),
    );

    assert_eq!(snapshots.get(12).expect("observed number").bytes, [12]);
    assert_eq!(snapshots.get(1).expect("restarted numbering").bytes, [1]);
    assert!(snapshots.get(2).is_none());

    let mut capped = PaneImageSnapshots::default();
    for number in 0..=u32::try_from(MAX_PANE_IMAGE_SNAPSHOTS).unwrap() {
        capped.insert(
            number,
            Arc::new(Image::from_bytes(
                ala::ImageFormat::Png,
                vec![u8::try_from(number).unwrap()],
            )),
        );
    }
    assert!(capped.get(0).is_none());
    assert!(capped.get(1).is_some());
    assert_eq!(capped.images.len(), MAX_PANE_IMAGE_SNAPSHOTS);
}

#[ala::test]
fn pasted_image_chunks_populate_replace_and_invalidate_the_daemon_number(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mux = test_mux(cx);
        mux.update(cx, |mux, cx| {
            let pane = PaneId(9);
            mux.begin_pasted_image(pane, 12, PastedImageFormat::Png, 4);
            mux.push_pasted_image_chunk(pane, 12, &[1, 2], cx);
            assert!(mux.pasted_image(pane, 12).is_none());
            mux.push_pasted_image_chunk(pane, 12, &[3, 4], cx);
            assert_eq!(
                mux.pasted_image(pane, 12).expect("assembled").bytes,
                [1, 2, 3, 4]
            );

            mux.begin_pasted_image(pane, 12, PastedImageFormat::Webp, 2);
            assert!(mux.pasted_image(pane, 12).is_none());
            mux.push_pasted_image_chunk(pane, 12, &[8, 9], cx);
            assert_eq!(
                mux.pasted_image(pane, 12).expect("replacement").bytes,
                [8, 9]
            );
            mux.pasted_image_unavailable(pane, 12);
            assert!(mux.pasted_image(pane, 12).is_none());

            mux.pending_pasted_image_previews.insert((pane, 13));
            mux.begin_pasted_image(pane, 13, PastedImageFormat::Png, 2);
            mux.push_pasted_image_chunk(pane, 13, &[1, 2, 3], cx);
            assert!(!mux.pasted_image_assemblies.contains_key(&(pane, 13)));
            assert!(!mux.pending_pasted_image_previews.contains(&(pane, 13)));
        });
    });
}

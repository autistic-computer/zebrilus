#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    macos::run()
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        error::Error,
        thread,
        time::{Duration, Instant},
    };

    use zz_simulator::{Button, Input, KeyDirection, Route, Simulator, TouchPhase, Turn};

    const USAGE: &str = "probe UDID [legacy|remote] screen SECONDS [swipe] | tap X Y | swipe X1 Y1 X2 Y2 | key USAGE... | button home|lock | hardware on|off | rotate left|right";
    const STEP: Duration = Duration::from_millis(8);

    pub(crate) fn run() -> Result<(), Box<dyn Error>> {
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        let [udid, rest @ ..] = arguments.as_slice() else {
            return Err(USAGE.into());
        };
        let (route, rest) = match rest {
            [route, rest @ ..] if route == "legacy" => (Some(Route::Legacy), rest),
            [route, rest @ ..] if route == "remote" => (Some(Route::Remote), rest),
            rest => (None, rest),
        };
        let [command, rest @ ..] = rest else {
            return Err(USAGE.into());
        };
        let simulator = Simulator::attach(udid)?;
        let open = || -> Result<Input, zz_simulator::Error> {
            let started = Instant::now();
            let input = match route {
                Some(route) => simulator.input_on(route)?,
                None => simulator.input()?,
            };
            println!(
                "input route {:?} open {:.1} ms",
                input.route(),
                started.elapsed().as_secs_f64() * 1000.0
            );
            Ok(input)
        };
        match (command.as_str(), rest) {
            ("screen", [seconds, flags @ ..]) => {
                let driver = flags.first().is_some_and(|flag| flag == "swipe").then(|| {
                    let udid = udid.clone();
                    let seconds: f64 = seconds.parse().unwrap_or(1.0);
                    thread::spawn(move || drive(&udid, Duration::from_secs_f64(seconds)))
                });
                screen(&simulator, Duration::from_secs_f64(seconds.parse()?))?;
                if let Some(driver) = driver {
                    driver.join().map_err(|_| "swipe thread panicked")??;
                }
            }
            ("tap", [x, y]) => {
                let input = open()?;
                input.touch(TouchPhase::Down, x.parse()?, y.parse()?)?;
                thread::sleep(Duration::from_millis(60));
                input.touch(TouchPhase::Up, x.parse()?, y.parse()?)?;
                let started = Instant::now();
                input.flush()?;
                println!("flush {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
                thread::sleep(Duration::from_millis(200));
            }
            ("swipe", [x1, y1, x2, y2]) => {
                let input = open()?;
                swipe(
                    &input,
                    (x1.parse()?, y1.parse()?),
                    (x2.parse()?, y2.parse()?),
                )?;
                input.flush()?;
                thread::sleep(Duration::from_millis(200));
            }
            ("key", usages) if !usages.is_empty() => {
                let input = open()?;
                for usage in usages {
                    let usage = u32::from_str_radix(usage.trim_start_matches("0x"), 16)?;
                    input.key(usage, KeyDirection::Down)?;
                    thread::sleep(Duration::from_millis(40));
                    input.key(usage, KeyDirection::Up)?;
                    thread::sleep(Duration::from_millis(40));
                }
                input.flush()?;
                thread::sleep(Duration::from_millis(200));
            }
            ("rotate", [direction]) => {
                let turn = if direction == "left" {
                    Turn::Left
                } else {
                    Turn::Right
                };
                simulator.rotate(turn)?;
                thread::sleep(Duration::from_millis(600));
                println!("turns {}", simulator.screen()?.turns());
            }
            ("hardware", [state]) => {
                simulator.set_hardware_keyboard(state == "on")?;
            }
            ("button", [name]) => {
                let button = match name.as_str() {
                    "home" => Button::Home,
                    "lock" => Button::Lock,
                    _ => return Err(USAGE.into()),
                };
                let input = open()?;
                input.button(button, KeyDirection::Down)?;
                thread::sleep(Duration::from_millis(60));
                input.button(button, KeyDirection::Up)?;
                input.flush()?;
                thread::sleep(Duration::from_millis(200));
            }
            _ => return Err(USAGE.into()),
        }
        Ok(())
    }

    fn screen(simulator: &Simulator, duration: Duration) -> Result<(), Box<dyn Error>> {
        let opened = Instant::now();
        let mut screen = simulator.screen()?;
        let surface = screen.surface();
        let format = surface.pixel_format.to_be_bytes();
        println!(
            "surface {}x{} format {} ({:#x}) displays {:?} open {:.1} ms",
            surface.width,
            surface.height,
            String::from_utf8_lossy(&format),
            surface.pixel_format,
            screen.displays(),
            opened.elapsed().as_secs_f64() * 1000.0,
        );
        let started = Instant::now();
        let mut seed = surface.seed;
        let mut seed_changes = 0_u64;
        let mut surface_changes = screen.surface_changes();
        while started.elapsed() < duration {
            thread::sleep(Duration::from_millis(1));
            if screen.surface_changes() != surface_changes {
                surface_changes = screen.surface_changes();
                screen.refresh()?;
            }
            let now = screen.surface().seed;
            if now != seed {
                seed = now;
                seed_changes += 1;
            }
        }
        let seconds = started.elapsed().as_secs_f64();
        println!(
            "callbacks {} ({:.1}/s) seed changes {} ({:.1}/s) surface changes {} over {:.2} s",
            screen.frames(),
            screen.frames() as f64 / seconds,
            seed_changes,
            seed_changes as f64 / seconds,
            screen.surface_changes(),
            seconds,
        );
        Ok(())
    }

    fn drive(udid: &str, duration: Duration) -> Result<(), String> {
        let simulator = Simulator::attach(udid).map_err(|error| error.to_string())?;
        let input = simulator.input().map_err(|error| error.to_string())?;
        let started = Instant::now();
        let mut forward = true;
        while started.elapsed() < duration {
            let (from, to) = if forward { (0.7, 0.3) } else { (0.3, 0.7) };
            swipe(&input, (0.5, from), (0.5, to)).map_err(|error| error.to_string())?;
            thread::sleep(Duration::from_millis(250));
            forward = !forward;
        }
        Ok(())
    }

    fn swipe(input: &Input, from: (f64, f64), to: (f64, f64)) -> Result<(), zz_simulator::Error> {
        const STEPS: u32 = 24;
        input.touch(TouchPhase::Down, from.0, from.1)?;
        for step in 1..=STEPS {
            let t = f64::from(step) / f64::from(STEPS);
            thread::sleep(STEP);
            input.touch(
                TouchPhase::Move,
                from.0 + (to.0 - from.0) * t,
                from.1 + (to.1 - from.1) * t,
            )?;
        }
        thread::sleep(STEP);
        input.touch(TouchPhase::Up, to.0, to.1)
    }
}

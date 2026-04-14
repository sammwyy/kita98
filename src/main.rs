mod audio;
mod bus;
mod cpu;
mod devices;
mod disk;
mod dos;
mod memory;
mod runtime;
mod video;

use anyhow::Result;
use clap::Parser;
use log::error;
use pixels::{Pixels, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

#[derive(Parser, Debug)]
#[command(name = "kita98", about = "PC-98-like x86 runtime")]
struct Cli {
    /// Path to the .hdi or .exe file
    file: String,

    /// Disable JIT and use interpreter only
    #[arg(short = 'i', long)]
    interp_only: bool,

    /// Log level
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let env = env_logger::Env::default().default_filter_or(&cli.log_level);
    env_logger::Builder::from_env(env)
        .format_timestamp(None)
        .init();

    log::info!("=== Kita98 Runtime v0.2 ===");

    let event_loop = EventLoop::new()?;
    // Leak the window so it has a 'static lifetime.
    // This solves the borrow checker issues with Pixels/Winit in the event loop.
    let window: &'static Window = Box::leak(Box::new(
        WindowBuilder::new()
            .with_title("Kita98 - PC-98 Emulator")
            .with_inner_size(LogicalSize::new(640.0, 400.0))
            .build(&event_loop)?,
    ));

    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, window);
        Pixels::new(640, 400, surface_texture)?
    };

    let mut rt = runtime::Runtime::new(&cli.file, false, 0, 0)?;

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                elwt.exit();
            }
            Event::AboutToWait => {
                for _ in 0..10_000 {
                    if let Err(e) = rt.step() {
                        error!("Runtime error: {}", e);
                        elwt.exit();
                        return;
                    }
                }
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                rt.bus.video.render(pixels.frame_mut());
                if let Err(err) = pixels.render() {
                    error!("pixels.render() failed: {}", err);
                    elwt.exit();
                }
            }
            _ => (),
        }
    })?;

    Ok(())
}

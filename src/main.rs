use winit::{
    event_loop::{ControlFlow, EventLoop},
};
use anyhow::Result;
use log::LevelFilter;
use fightish::App;

fn main() -> Result<()>{
    env_logger::builder()
        .filter(Some("wgpu_hal"), LevelFilter::Warn)
        .filter(Some("wgpu_core"), LevelFilter::Warn)
        .init();
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

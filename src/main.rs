//! Fuentes de BookOS: ver, instalar, activar y quitar tipografías.

mod catalogo;
mod entrada;
mod tema;
mod textos;
mod vista;

use std::borrow::Cow;
use std::path::PathBuf;

use gpui::{
    App, AssetSource, Bounds, SharedString, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowOptions, prelude::*, px, size,
};

/// Los iconos van dentro del binario: son una docena de SVG pequeños y así no
/// hay que instalarlos ni buscarlos en disco.
struct Iconos;

impl AssetSource for Iconos {
    fn load(&self, ruta: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        macro_rules! iconos {
            ($($n:literal),*) => {
                match ruta {
                    $(concat!("iconos/", $n, ".svg") => Some(include_bytes!(concat!("../assets/iconos/", $n, ".svg")).as_slice()),)*
                    _ => None,
                }
            };
        }
        Ok(iconos!(
            "arrow-down-tray",
            "check-circle",
            "chevron-right",
            "exclamation-circle",
            "folder-open",
            "magnifying-glass",
            "plus",
            "x-mark",
            "ventana-cerrar",
            "ventana-maximizar",
            "ventana-minimizar",
            "ventana-restaurar"
        )
        .map(Cow::Borrowed))
    }

    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

fn main() {
    // Archivos pasados al abrir una fuente desde Archivos (`%F` del .desktop).
    let archivos: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    gpui_platform::application()
        .with_assets(Iconos)
        .run(move |cx: &mut App| {
            cx.bind_keys(entrada::atajos());
            cx.bind_keys(vista::atajos());
            let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    app_id: Some("bookos-fonts".into()),
                    // La barra la dibuja la app, como el resto de apps de BookOS:
                    // la del escritorio iría separada del fondo de la ventana.
                    window_decorations: Some(WindowDecorations::Client),
                    window_background: WindowBackgroundAppearance::Transparent,
                    window_min_size: Some(size(px(860.), px(560.))),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| vista::Fuentes::new(archivos, window, cx)),
            )
            .expect("el compositor acepta la ventana");
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            cx.activate(true);
        });
}

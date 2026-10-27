//! Colores de la ventana: la paleta dinámica de Ajustes o, sin ella, el
//! respaldo fijo de la guía (AI-DESIGN-SYSTEM.md §2.1).
//!
//! Ajustes escribe la misma paleta en `palette.css` y en `palette.json`. Aquí
//! no hay motor CSS, así que se lee el JSON, que la guía da como formato para
//! lo que no consume CSS.

use std::fs;
use std::path::PathBuf;

use gpui::{Hsla, Rgba};

#[derive(Clone, Copy)]
pub struct Tema {
    pub oscuro: bool,
    pub bg: Hsla,
    pub card: Hsla,
    pub sbg: Hsla,
    pub tx: Hsla,
    pub tx2: Hsla,
    pub div: Hsla,
    pub brd: Hsla,
    pub hov: Hsla,
    pub act: Hsla,
    pub blue: Hsla,
    pub toff: Hsla,
    pub on_accent: Hsla,
    // Los de estado no salen del fondo: son fijos en las dos paletas.
    pub red: Hsla,
}

impl Tema {
    pub fn cargar(oscuro: bool) -> Tema {
        let mut tema = if oscuro {
            RESPALDO_OSCURO
        } else {
            RESPALDO_CLARO
        };
        if let Ok(json) = fs::read_to_string(config("palette.json")) {
            tema.aplicar(&json, if oscuro { "dark" } else { "light" });
        }
        tema
    }

    fn aplicar(&mut self, json: &str, modo: &str) {
        let Some(objeto) = json
            .split_once(&format!("\"{modo}\":{{"))
            .and_then(|(_, resto)| resto.split_once('}'))
            .map(|(objeto, _)| objeto)
        else {
            return;
        };
        // Partido por comillas queda «clave», «:», «valor», «,», «clave»…:
        // los valores no llevan comillas, aunque `rgba(…)` sí lleve comas.
        let trozos: Vec<&str> = objeto.split('"').collect();
        for par in trozos.get(1..).unwrap_or_default().chunks(4) {
            let [clave, _, valor, ..] = par else { continue };
            let Some(color) = color(valor) else { continue };
            match *clave {
                "bg" => self.bg = color,
                "card" => self.card = color,
                "sbg" => self.sbg = color,
                "tx" => self.tx = color,
                "tx2" => self.tx2 = color,
                "div" => self.div = color,
                "brd" => self.brd = color,
                "hov" => self.hov = color,
                "act" => self.act = color,
                "blue" => self.blue = color,
                "toff" => self.toff = color,
                "on-accent" => self.on_accent = color,
                _ => {}
            }
        }
    }

    /// Fondo de lo seleccionado en una lista: el acento al 10 %, nunca sólido.
    pub fn seleccion(&self) -> Hsla {
        self.blue.opacity(0.10)
    }

    /// Reposo de las tarjetas; en oscuro no llevan sombra.
    pub fn sombra_reposo(&self) -> Vec<gpui::BoxShadow> {
        if self.oscuro {
            return Vec::new();
        }
        vec![sombra(1.0, 3.0, 0.05)]
    }
}

pub fn sombra(y: f32, blur: f32, alfa: f32) -> gpui::BoxShadow {
    gpui::BoxShadow {
        color: gpui::black().opacity(alfa),
        offset: gpui::point(gpui::px(0.), gpui::px(y)),
        blur_radius: gpui::px(blur),
        spread_radius: gpui::px(0.),
    }
}

pub fn config(nombre: &str) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("bookos")
        .join(nombre)
}

fn color(texto: &str) -> Option<Hsla> {
    let texto = texto.trim();
    if let Some(hex) = texto.strip_prefix('#') {
        let n = u32::from_str_radix(hex, 16).ok()?;
        return match hex.len() {
            6 => Some(gpui::rgb(n).into()),
            8 => Some(gpui::rgba(n).into()),
            _ => None,
        };
    }
    let dentro = texto.strip_prefix("rgba(")?.strip_suffix(')')?;
    let mut partes = dentro.split(',').map(|p| p.trim().parse::<f32>());
    let mut siguiente = || partes.next()?.ok();
    let (r, g, b, a) = (siguiente()?, siguiente()?, siguiente()?, siguiente()?);
    Some(
        Rgba {
            r: r / 255.,
            g: g / 255.,
            b: b / 255.,
            a,
        }
        .into(),
    )
}

const fn gris(l: f32, a: f32) -> Hsla {
    Hsla { h: 0., s: 0., l, a }
}

/// `#rrggbb` en tiempo de compilación, para poder tener las paletas en `const`.
const fn hex(n: u32) -> Hsla {
    let r = ((n >> 16) & 0xFF) as f32 / 255.;
    let g = ((n >> 8) & 0xFF) as f32 / 255.;
    let b = (n & 0xFF) as f32 / 255.;
    let max = if r > g {
        if r > b { r } else { b }
    } else if g > b {
        g
    } else {
        b
    };
    let min = if r < g {
        if r < b { r } else { b }
    } else if g < b {
        g
    } else {
        b
    };
    let l = (max + min) / 2.;
    if max == min {
        return gris(l, 1.);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2. - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6. } else { 0. }
    } else if max == g {
        (b - r) / d + 2.
    } else {
        (r - g) / d + 4.
    };
    Hsla {
        h: h / 6.,
        s,
        l,
        a: 1.,
    }
}

const RESPALDO_CLARO: Tema = Tema {
    oscuro: false,
    bg: hex(0xf2f2f7),
    card: hex(0xffffff),
    sbg: gris(0., 0.06),
    tx: hex(0x000000),
    tx2: hex(0x8e8e93),
    div: gris(0., 0.08),
    brd: gris(0., 0.10),
    hov: gris(0., 0.04),
    act: gris(0., 0.08),
    blue: hex(0x007aff),
    toff: hex(0xe0e0e0),
    on_accent: hex(0xffffff),
    red: hex(0xff3b30),
};

const RESPALDO_OSCURO: Tema = Tema {
    oscuro: true,
    bg: hex(0x000000),
    card: hex(0x1c1c1e),
    sbg: gris(1., 0.08),
    tx: hex(0xffffff),
    tx2: hex(0x8e8e93),
    div: gris(1., 0.08),
    brd: gris(1., 0.06),
    hov: gris(1., 0.06),
    act: gris(1., 0.10),
    blue: hex(0x0a84ff),
    toff: hex(0x48484a),
    on_accent: hex(0xffffff),
    red: hex(0xff453a),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_colores_de_la_paleta() {
        let json = r##"{"seed":"#635B50","light":{"bg":"#F8F3EB","sbg":"rgba(124,93,46,0.08)","blue":"#7C5D2E"},"dark":{"bg":"#16130D","blue":"#E3C090"}}"##;
        let mut claro = RESPALDO_CLARO;
        claro.aplicar(json, "light");
        assert_eq!(claro.bg, color("#F8F3EB").expect("hex"));
        assert_eq!(claro.blue, color("#7C5D2E").expect("hex"));
        assert!((claro.sbg.a - 0.08).abs() < 1e-6);
        // Lo que la paleta no trae se queda con el respaldo.
        assert_eq!(claro.card, RESPALDO_CLARO.card);
        let mut oscuro = RESPALDO_OSCURO;
        oscuro.aplicar(json, "dark");
        assert_eq!(oscuro.bg, color("#16130D").expect("hex"));
    }

    #[test]
    fn un_json_roto_deja_el_respaldo() {
        let mut t = RESPALDO_CLARO;
        t.aplicar("{\"light\":", "light");
        assert_eq!(t.bg, RESPALDO_CLARO.bg);
    }

    #[test]
    fn el_hex_constante_coincide_con_el_de_gpui() {
        for n in [0xf2f2f7, 0x007aff, 0xff3b30, 0x8e8e93, 0x1c1c1e] {
            let a = hex(n);
            let b: Hsla = gpui::rgb(n).into();
            assert!(
                (a.h - b.h).abs() < 1e-4 && (a.s - b.s).abs() < 1e-4 && (a.l - b.l).abs() < 1e-4,
                "{n:x}"
            );
        }
    }
}

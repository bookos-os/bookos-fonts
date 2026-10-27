//! Qué fuentes hay, de dónde salen y qué se puede hacer con ellas.
//!
//! La fuente de verdad es fontconfig y no la base de datos de GPUI: es lo que
//! usan GTK, Qt y WebKit, así que lo que aquí sale como instalado o
//! desactivado es lo que ven las demás aplicaciones. Se le pregunta con
//! `fc-list` en vez de enlazar libfontconfig: con la caché ya hecha contesta
//! en unos 20 ms y no deja nada residente en el proceso.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use gpui::SharedString;

use crate::textos::Textos;

/// De dónde viene una familia, que decide qué se puede hacer con ella.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origen {
    /// En la carpeta del usuario: se puede borrar.
    Usuario,
    /// Instalada por paquetes: es del sistema, solo se puede desactivar.
    Sistema,
    /// Un archivo abierto para verlo antes de instalarlo.
    Archivo,
}

#[derive(Clone, Debug)]
pub struct Estilo {
    pub nombre: SharedString,
    pub archivo: PathBuf,
    /// Índice de la cara dentro del archivo (colecciones `.ttc`).
    pub indice: u32,
    /// Peso en la escala CSS (100–900).
    pub peso: f32,
    pub cursiva: bool,
}

#[derive(Clone, Debug)]
pub struct Familia {
    pub nombre: SharedString,
    /// El nombre en minúsculas, para ordenar y buscar sin recalcularlo en cada
    /// tecla.
    pub clave: String,
    pub estilos: Vec<Estilo>,
    pub origen: Origen,
    pub mono: bool,
    pub desactivada: bool,
}

impl Familia {
    pub fn archivos(&self) -> BTreeSet<&Path> {
        self.estilos.iter().map(|e| e.archivo.as_path()).collect()
    }
}

#[derive(Default)]
pub struct Catalogo {
    pub familias: Vec<Familia>,
    /// Familias que fontconfig da para `sans-serif`, `serif` y `monospace`.
    /// Desactivarlas dejaría a las aplicaciones sin letra por defecto.
    pub protegidas: Vec<String>,
}

#[derive(Debug)]
pub enum Fallo {
    NoEsFuente(String),
    YaInstalada(String),
    Papelera(String),
    Herramienta(&'static str, String),
    Disco(String),
}

impl Fallo {
    pub fn mensaje(&self, t: &Textos) -> String {
        match self {
            Fallo::NoEsFuente(archivo) => t.err_no_es_fuente.replace("{}", archivo),
            Fallo::YaInstalada(archivo) => t.err_ya_instalada.replace("{}", archivo),
            Fallo::Papelera(detalle) => t.err_papelera.replace("{}", detalle),
            Fallo::Herramienta(orden, detalle) => t
                .err_herramienta
                .replace("{orden}", orden)
                .replace("{}", detalle),
            Fallo::Disco(detalle) => t.err_disco.replace("{}", detalle),
        }
    }
}

fn disco(e: std::io::Error) -> Fallo {
    Fallo::Disco(e.to_string())
}

/// `%{family[0]}`: fontconfig pone primero el nombre tipográfico (ID 16 de la
/// tabla `name`), igual que fontdb, que es la base de datos de GPUI. Con eso el
/// nombre que sale aquí es el mismo con el que GPUI encuentra la fuente.
const FORMATO: &str =
    "%{family[0]}\t%{style[0]}\t%{file}\t%{index}\t%{weight}\t%{slant}\t%{spacing}\t%{variable}\n";

pub fn carpeta_usuario() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
        .join("fonts")
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn conf_desactivadas() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("fontconfig/conf.d/90-bookos-desactivadas.conf")
}

fn ejecutar(programa: &'static str, args: &[&str]) -> Result<String, Fallo> {
    let salida = Command::new(programa)
        .args(args)
        .output()
        .map_err(|e| Fallo::Herramienta(programa, e.to_string()))?;
    if !salida.status.success() {
        let error = String::from_utf8_lossy(&salida.stderr).trim().to_string();
        return Err(Fallo::Herramienta(programa, error));
    }
    Ok(String::from_utf8_lossy(&salida.stdout).into_owned())
}

pub fn cargar() -> Result<Catalogo, Fallo> {
    let instaladas = ejecutar("fc-list", &["--format", FORMATO])?;
    let desactivadas = leer_desactivadas();
    let mut caras: Vec<Cara> = instaladas.lines().filter_map(Cara::leer).collect();

    // fontconfig no lista lo que rechaza su configuración, así que las
    // desactivadas se leen archivo a archivo; `fc-query` no mira la
    // configuración.
    let existentes: Vec<String> = desactivadas
        .iter()
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if !existentes.is_empty() {
        let mut args = vec!["--format", FORMATO];
        args.extend(existentes.iter().map(String::as_str));
        caras.extend(ejecutar("fc-query", &args)?.lines().filter_map(Cara::leer));
    }

    let protegidas = ["sans-serif", "serif", "monospace"]
        .iter()
        .filter_map(|generica| ejecutar("fc-match", &["-f", "%{family[0]}", generica]).ok())
        .collect();

    Ok(Catalogo {
        familias: agrupar(caras, &desactivadas, &carpeta_usuario(), &home()),
        protegidas,
    })
}

/// Una línea de `fc-list` ya separada.
#[derive(Debug, PartialEq)]
struct Cara {
    familia: String,
    estilo: String,
    archivo: PathBuf,
    indice: u32,
    peso: f32,
    cursiva: bool,
    mono: bool,
    variable: bool,
}

impl Cara {
    fn leer(linea: &str) -> Option<Cara> {
        let mut c = linea.split('\t');
        let familia = c.next()?.trim();
        let estilo = c.next()?.trim();
        let archivo = c.next()?;
        let indice: u32 = c.next()?.parse().ok()?;
        let peso = c.next()?;
        let slant: u32 = c.next()?.parse().unwrap_or(0);
        let spacing: u32 = c.next()?.parse().unwrap_or(0);
        let variable = c.next()? == "True";
        if familia.is_empty() {
            return None;
        }
        Some(Cara {
            familia: familia.to_string(),
            estilo: estilo.to_string(),
            archivo: PathBuf::from(archivo),
            // La entrada «maestra» de una variable trae un rango («[0 210]»):
            // no es un estilo, se descarta al agrupar si hay instancias.
            peso: peso.parse().map(peso_css).unwrap_or(400.0),
            indice,
            cursiva: slant >= 100,
            mono: spacing >= 100,
            variable,
        })
    }
}

/// Escala de fontconfig a la de CSS, por tramos como `FcWeightToOpenType`.
fn peso_css(fc: f32) -> f32 {
    const TABLA: [(f32, f32); 11] = [
        (0.0, 100.0),
        (40.0, 200.0),
        (50.0, 300.0),
        (55.0, 350.0),
        (75.0, 380.0),
        (80.0, 400.0),
        (100.0, 500.0),
        (180.0, 600.0),
        (200.0, 700.0),
        (205.0, 800.0),
        (210.0, 900.0),
    ];
    if fc >= 210.0 {
        return 900.0;
    }
    TABLA
        .windows(2)
        .find(|t| fc <= t[1].0)
        .map(|t| {
            let ((a, ca), (b, cb)) = (t[0], t[1]);
            ca + (fc - a) / (b - a) * (cb - ca)
        })
        .unwrap_or(400.0)
}

fn agrupar(
    mut caras: Vec<Cara>,
    desactivadas: &BTreeSet<PathBuf>,
    usuario: &Path,
    home: &Path,
) -> Vec<Familia> {
    caras.sort_by(|a, b| {
        a.familia
            .to_lowercase()
            .cmp(&b.familia.to_lowercase())
            // Desempate exacto: sin él, «Cantarell» y «cantarell» quedarían
            // intercaladas y saldrían partidas en varias familias.
            .then(a.familia.cmp(&b.familia))
            .then(a.variable.cmp(&b.variable))
            .then(a.indice.cmp(&b.indice))
    });
    let mut familias: Vec<Familia> = Vec::new();
    for cara in caras {
        let origen = if cara.archivo.starts_with(usuario) || cara.archivo.starts_with(home) {
            Origen::Usuario
        } else {
            Origen::Sistema
        };
        let desactivada = desactivadas.contains(&cara.archivo);
        let familia = match familias.last_mut() {
            Some(f) if f.nombre.as_ref() == cara.familia => f,
            _ => {
                familias.push(Familia {
                    clave: cara.familia.to_lowercase(),
                    nombre: cara.familia.clone().into(),
                    estilos: Vec::new(),
                    origen,
                    mono: cara.mono,
                    desactivada: true,
                });
                familias.last_mut().expect("recién añadida")
            }
        };
        // Un solo archivo del sistema basta para que no se pueda borrar.
        if origen == Origen::Sistema {
            familia.origen = Origen::Sistema;
        }
        familia.desactivada &= desactivada;
        // La variable llega detrás de las estáticas por el orden de arriba:
        // si ya hay estilos, su entrada maestra sobra, y un estilo repetido
        // (Cantarell instalada estática y variable) se queda con el primero.
        let nombre = if cara.estilo.is_empty() {
            "Regular"
        } else {
            cara.estilo.as_str()
        };
        if (cara.variable && !familia.estilos.is_empty())
            || familia.estilos.iter().any(|e| e.nombre.as_ref() == nombre)
        {
            continue;
        }
        familia.estilos.push(Estilo {
            nombre: nombre.to_string().into(),
            archivo: cara.archivo,
            indice: cara.indice,
            peso: cara.peso,
            cursiva: cara.cursiva,
        });
    }
    for familia in &mut familias {
        familia
            .estilos
            .sort_by(|a, b| a.cursiva.cmp(&b.cursiva).then(a.peso.total_cmp(&b.peso)));
    }
    familias
}

/// Lee un archivo suelto para enseñarlo antes de instalarlo.
pub fn leer_archivo(archivo: &Path) -> Result<Vec<Familia>, Fallo> {
    let ruta = archivo.to_string_lossy();
    let nombre = || nombre_archivo(archivo);
    let salida = ejecutar("fc-query", &["--format", FORMATO, &ruta])
        .map_err(|_| Fallo::NoEsFuente(nombre()))?;
    let caras: Vec<Cara> = salida.lines().filter_map(Cara::leer).collect();
    if caras.is_empty() {
        return Err(Fallo::NoEsFuente(nombre()));
    }
    let mut familias = agrupar(caras, &BTreeSet::new(), Path::new("/-"), Path::new("/-"));
    for f in &mut familias {
        f.origen = Origen::Archivo;
        f.desactivada = false;
    }
    Ok(familias)
}

fn nombre_archivo(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

pub fn es_archivo_de_fuente(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["ttf", "otf", "ttc", "otc"]
            .iter()
            .any(|x| e.eq_ignore_ascii_case(x))
    })
}

/// Copia los archivos a la carpeta del usuario y avisa a fontconfig.
///
/// Devuelve el contenido de cada archivo instalado para registrarlo en GPUI:
/// su base de datos se llenó al arrancar y no vuelve a mirar las carpetas.
pub fn instalar(archivos: &[PathBuf]) -> Result<Vec<Vec<u8>>, Fallo> {
    let destino = carpeta_usuario();
    fs::create_dir_all(&destino).map_err(disco)?;
    // Primero se valida todo: instalar la mitad de una selección y fallar en
    // la otra mitad deja al usuario sin saber qué entró.
    let mut leidos = Vec::with_capacity(archivos.len());
    for archivo in archivos {
        let datos = fs::read(archivo).map_err(disco)?;
        let caras = ttf_parser::fonts_in_collection(&datos).unwrap_or(1);
        if caras == 0 || ttf_parser::Face::parse(&datos, 0).is_err() {
            return Err(Fallo::NoEsFuente(nombre_archivo(archivo)));
        }
        let final_ = destino.join(
            archivo
                .file_name()
                .ok_or_else(|| Fallo::NoEsFuente(nombre_archivo(archivo)))?,
        );
        if final_.exists() {
            return Err(Fallo::YaInstalada(nombre_archivo(archivo)));
        }
        leidos.push((final_, datos));
    }
    for (final_, datos) in &leidos {
        fs::write(final_, datos).map_err(disco)?;
    }
    ejecutar("fc-cache", &["-f", &destino.to_string_lossy()])?;
    Ok(leidos.into_iter().map(|(_, datos)| datos).collect())
}

/// Manda a la papelera los archivos de una familia del usuario. A la papelera
/// y no `remove_file`: así se puede deshacer desde Archivos.
pub fn eliminar(familia: &Familia) -> Result<(), Fallo> {
    let archivos: Vec<String> = familia
        .archivos()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let mut args = vec!["trash"];
    args.extend(archivos.iter().map(String::as_str));
    ejecutar("gio", &args).map_err(|e| match e {
        // `gio` cuenta el motivo con la URI delante: se deja solo el final,
        // que es lo que se entiende («Permiso denegado»).
        Fallo::Herramienta(_, detalle) => {
            Fallo::Papelera(detalle.rsplit(": ").next().unwrap_or(&detalle).to_string())
        }
        otro => otro,
    })?;
    // Si estaba desactivada, su regla ya no apunta a nada.
    let mut lista = leer_desactivadas();
    let antes = lista.len();
    lista.retain(|p| !familia.archivos().contains(p.as_path()));
    if lista.len() != antes {
        escribir_desactivadas(&lista)?;
    }
    ejecutar("fc-cache", &["-f", &carpeta_usuario().to_string_lossy()])?;
    Ok(())
}

/// Activa o desactiva una familia para las aplicaciones que usan fontconfig.
///
/// Es un `<rejectfont>` por archivo en la configuración del usuario: no toca
/// los archivos, así que reactivar es inmediato y funciona también con las
/// fuentes del sistema. Las apps abiertas lo notan al reiniciarse.
pub fn desactivar(familia: &Familia, desactivar: bool) -> Result<(), Fallo> {
    let mut lista = leer_desactivadas();
    for archivo in familia.archivos() {
        if desactivar {
            lista.insert(archivo.to_path_buf());
        } else {
            lista.remove(archivo);
        }
    }
    escribir_desactivadas(&lista)
}

fn leer_desactivadas() -> BTreeSet<PathBuf> {
    fs::read_to_string(conf_desactivadas())
        .map(|xml| leer_globs(&xml))
        .unwrap_or_default()
}

fn leer_globs(xml: &str) -> BTreeSet<PathBuf> {
    xml.split("<glob>")
        .skip(1)
        .filter_map(|trozo| trozo.split_once("</glob>"))
        .map(|(ruta, _)| PathBuf::from(desescapar(ruta)))
        .collect()
}

fn escribir_desactivadas(lista: &BTreeSet<PathBuf>) -> Result<(), Fallo> {
    let ruta = conf_desactivadas();
    if lista.is_empty() {
        return match fs::remove_file(&ruta) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(disco(e)),
            _ => Ok(()),
        };
    }
    fs::create_dir_all(ruta.parent().expect("la ruta de la conf tiene carpeta")).map_err(disco)?;
    fs::write(&ruta, generar_conf(lista)).map_err(disco)
}

fn generar_conf(lista: &BTreeSet<PathBuf>) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n\
         <!-- Lo escribe Fuentes de BookOS: fuentes desactivadas. -->\n\
         <fontconfig>\n  <selectfont>\n    <rejectfont>\n",
    );
    for p in lista {
        xml.push_str("      <glob>");
        xml.push_str(&escapar(&p.to_string_lossy()));
        xml.push_str("</glob>\n");
    }
    xml.push_str("    </rejectfont>\n  </selectfont>\n</fontconfig>\n");
    xml
}

// El `*` y el `?` de una ruta también son comodines para fontconfig, pero un
// archivo de fuente con esos caracteres en el nombre no se ha visto nunca.
fn escapar(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn desescapar(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Lo que se enseña de una cara: sale de sus tablas, no de fontconfig.
#[derive(Default)]
pub struct Ficha {
    pub version: Option<String>,
    pub disenador: Option<String>,
    pub fabricante: Option<String>,
    pub copyright: Option<String>,
    pub licencia: Option<String>,
    pub formato: &'static str,
    pub glifos: u16,
    pub ejes: Vec<String>,
    pub tamano: u64,
    /// Los primeros caracteres que dibuja, para la rejilla.
    pub caracteres: Vec<char>,
    /// GPUI descarta en silencio las caras sin «m» (fuentes de símbolos) y
    /// dibuja con otra: sin esto la vista previa mentiría.
    pub tiene_latin: bool,
}

const MAX_CARACTERES: usize = 160;

pub fn ficha(estilo: &Estilo) -> Result<Ficha, Fallo> {
    let datos = fs::read(&estilo.archivo).map_err(disco)?;
    let indice = estilo.indice & 0xFFFF;
    let cara = ttf_parser::Face::parse(&datos, indice)
        .map_err(|_| Fallo::NoEsFuente(nombre_archivo(&estilo.archivo)))?;
    let nombre = |id| {
        let mut nombres = cara.names().into_iter().filter(|n| n.name_id == id);
        // Inglés de Windows primero; si no, el primero que se pueda leer.
        let ingles = nombres
            .clone()
            .find(|n| n.language_id == 0x409)
            .and_then(|n| n.to_string());
        ingles
            .or_else(|| nombres.find_map(|n| n.to_string()))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let tablas = cara.tables();
    let formato = if tablas.cff.is_some() || tablas.cff2.is_some() {
        "OpenType (CFF)"
    } else {
        "TrueType"
    };

    let mut puntos = BTreeSet::new();
    if let Some(cmap) = tablas.cmap {
        for sub in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
            sub.codepoints(|c| {
                puntos.insert(c);
            });
        }
    }
    let caracteres = puntos
        .into_iter()
        .filter_map(char::from_u32)
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .filter(|c| !('\u{E000}'..='\u{F8FF}').contains(c))
        // Los de formato (guion blando, espacios de anchura cero) no dibujan
        // nada y dejarían casillas vacías.
        .filter(|c| !matches!(c, '\u{AD}' | '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}'))
        .take(MAX_CARACTERES)
        .collect();

    Ok(Ficha {
        version: nombre(ttf_parser::name_id::VERSION),
        disenador: nombre(ttf_parser::name_id::DESIGNER),
        fabricante: nombre(ttf_parser::name_id::MANUFACTURER),
        copyright: nombre(ttf_parser::name_id::COPYRIGHT_NOTICE),
        licencia: nombre(ttf_parser::name_id::LICENSE),
        formato,
        glifos: cara.number_of_glyphs(),
        ejes: cara
            .variation_axes()
            .into_iter()
            .map(|e| e.tag.to_string())
            .collect(),
        tamano: datos.len() as u64,
        caracteres,
        tiene_latin: cara.glyph_index('m').is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cara(linea: &str) -> Cara {
        Cara::leer(linea).expect("línea válida")
    }

    #[test]
    fn lee_una_linea_de_fc_list() {
        let c = cara("Adwaita Sans\tBold Italic\t/usr/a.ttf\t458752\t200\t100\t\tFalse");
        assert_eq!(c.familia, "Adwaita Sans");
        assert_eq!(c.estilo, "Bold Italic");
        assert_eq!(c.peso, 700.0);
        assert!(c.cursiva);
        assert!(!c.mono);
        assert!(!c.variable);
    }

    #[test]
    fn la_entrada_maestra_de_una_variable_no_rompe_el_peso() {
        let c = cara("Cantarell\t\t/usr/vf.otf\t0\t[0 205]\t0\t\tTrue");
        assert_eq!(c.peso, 400.0);
        assert!(c.variable);
    }

    #[test]
    fn convierte_pesos_de_fontconfig_a_css() {
        assert_eq!(peso_css(0.0), 100.0);
        assert_eq!(peso_css(80.0), 400.0);
        assert_eq!(peso_css(180.0), 600.0);
        assert_eq!(peso_css(140.0), 550.0);
        assert_eq!(peso_css(215.0), 900.0);
    }

    #[test]
    fn agrupa_por_familia_y_quita_duplicados() {
        let caras = vec![
            cara("Cantarell\tBold\t/usr/c/Cantarell-Bold.otf\t0\t200\t0\t\tFalse"),
            cara("cantarell\t\t/usr/c/VF.otf\t0\t[0 205]\t0\t\tTrue"),
            cara("Cantarell\tBold\t/usr/c/VF.otf\t262144\t200\t0\t\tFalse"),
            cara("Cantarell\tRegular\t/usr/c/Cantarell-Regular.otf\t0\t80\t0\t\tFalse"),
            cara("Mía\tRegular\t/home/u/.local/share/fonts/m.ttf\t0\t80\t0\t100\tFalse"),
        ];
        let f = agrupar(
            caras,
            &BTreeSet::new(),
            Path::new("/home/u/.local/share/fonts"),
            Path::new("/home/u"),
        );
        let nombres: Vec<_> = f.iter().map(|f| f.nombre.to_string()).collect();
        // «cantarell» en minúsculas es otra familia para fontconfig; se
        // respeta tal cual llega.
        assert_eq!(nombres, ["Cantarell", "cantarell", "Mía"]);
        let estilos: Vec<_> = f[0].estilos.iter().map(|e| e.nombre.to_string()).collect();
        assert_eq!(estilos, ["Regular", "Bold"]);
        assert_eq!(f[0].origen, Origen::Sistema);
        assert_eq!(f[2].origen, Origen::Usuario);
        assert!(f[2].mono);
    }

    #[test]
    fn una_variable_sin_instancias_se_queda_con_su_entrada() {
        let f = agrupar(
            vec![cara("Sola\t\t/usr/s.ttf\t0\t[0 210]\t0\t\tTrue")],
            &BTreeSet::new(),
            Path::new("/x"),
            Path::new("/x"),
        );
        assert_eq!(f[0].estilos.len(), 1);
        assert_eq!(f[0].estilos[0].nombre.as_ref(), "Regular");
    }

    #[test]
    fn una_familia_solo_esta_desactivada_si_lo_estan_todos_sus_archivos() {
        let caras = || {
            vec![
                cara("F\tRegular\t/usr/f/r.ttf\t0\t80\t0\t\tFalse"),
                cara("F\tBold\t/usr/f/b.ttf\t0\t200\t0\t\tFalse"),
            ]
        };
        let una: BTreeSet<PathBuf> = [PathBuf::from("/usr/f/r.ttf")].into();
        assert!(!agrupar(caras(), &una, Path::new("/x"), Path::new("/x"))[0].desactivada);
        let dos: BTreeSet<PathBuf> =
            [PathBuf::from("/usr/f/r.ttf"), PathBuf::from("/usr/f/b.ttf")].into();
        assert!(agrupar(caras(), &dos, Path::new("/x"), Path::new("/x"))[0].desactivada);
    }

    #[test]
    fn la_conf_de_desactivadas_se_lee_como_se_escribe() {
        let lista: BTreeSet<PathBuf> = [
            PathBuf::from("/usr/a & b/<x>.ttf"),
            PathBuf::from("/home/u/c.otf"),
        ]
        .into();
        assert_eq!(leer_globs(&generar_conf(&lista)), lista);
    }

    #[test]
    fn reconoce_extensiones_de_fuente() {
        assert!(es_archivo_de_fuente(Path::new("/a/B.TTF")));
        assert!(es_archivo_de_fuente(Path::new("x.otc")));
        assert!(!es_archivo_de_fuente(Path::new("x.woff2")));
        assert!(!es_archivo_de_fuente(Path::new("ttf")));
    }

    #[test]
    fn lee_la_ficha_de_una_fuente_real() {
        let catalogo = cargar().expect("fontconfig instalado");
        let familia = catalogo
            .familias
            .iter()
            .find(|f| catalogo.protegidas.contains(&f.nombre.to_string()))
            .expect("la familia por defecto está instalada");
        let ficha = ficha(&familia.estilos[0]).expect("se puede leer");
        assert!(ficha.tiene_latin);
        assert!(ficha.glifos > 100);
        assert!(ficha.caracteres.contains(&'A'));
    }
}

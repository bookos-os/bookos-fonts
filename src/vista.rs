//! La ventana: barra de título, lateral con las familias y detalle de la
//! elegida. Medidas y colores de AI-DESIGN-SYSTEM.md; donde GPUI no llega
//! (letter-spacing, desenfoque, transformaciones) se dice en su sitio.

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Bounds, ClickEvent, Context, CursorStyle, Decorations,
    Entity, ExternalPaths, FocusHandle, Focusable, FontWeight, Hsla, KeyBinding, MouseButton,
    MouseDownEvent, MouseMoveEvent, PathPromptOptions, Pixels, ResizeEdge, ScrollHandle,
    ScrollStrategy, SharedString, Size, Subscription, UniformListScrollHandle, Window, actions,
    canvas, div, point, prelude::*, px, svg, uniform_list,
};

use crate::catalogo::{self, Catalogo, Estilo, Fallo, Familia, Ficha, Origen};
use crate::entrada::{Cambio, Entrada};
use crate::tema::{self, Tema};
use crate::textos::Textos;

actions!(fuentes, [Escape, Arriba, Abajo, Buscar, Cerrar]);

pub fn atajos() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", Escape, None),
        KeyBinding::new("up", Arriba, Some("Lista")),
        KeyBinding::new("down", Abajo, Some("Lista")),
        KeyBinding::new("secondary-f", Buscar, None),
        KeyBinding::new("secondary-w", Cerrar, None),
        KeyBinding::new("secondary-q", Cerrar, None),
    ]
}

/// Lo que GPUI deja fuera de la ventana para agarrarla por el borde y
/// cambiarle el tamaño. El escritorio no lo cuenta como ventana.
const BORDE: Pixels = px(8.);
const FUENTE_UI: &str = "Noto Sans";
const ALTO_FILA: Pixels = px(58.);

// Curvas de la guía (§2.7).
const DURACION_PAGINA: Duration = Duration::from_millis(140);
const DURACION_POPOVER: Duration = Duration::from_millis(180);
const DURACION_TOGGLE: Duration = Duration::from_millis(250);
const VIDA_TOAST: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filtro {
    Todas,
    Mias,
    Sistema,
}

enum Estado {
    Cargando,
    Listo,
    Error(SharedString),
}

/// Lo que enseña el detalle.
enum Seleccion {
    /// Por nombre y no por índice: el índice cambia cada vez que se recarga
    /// el catálogo.
    Instalada(SharedString),
    /// Archivos abiertos o soltados en la ventana, aún sin instalar.
    Archivos {
        familias: Vec<Familia>,
        archivos: Vec<PathBuf>,
    },
}

/// Lo que se puede repetir desde la tarjeta de error.
#[derive(Clone)]
enum Operacion {
    Instalar(Vec<PathBuf>),
    Activar(bool),
    Eliminar,
    Abrir(Vec<PathBuf>),
}

struct ErrorPagina {
    mensaje: SharedString,
    reintento: Option<Operacion>,
}

enum EstadoFicha {
    Cargando,
    Lista(Ficha),
    Error(SharedString),
}

pub struct Fuentes {
    t: &'static Textos,
    tema: Tema,
    estado: Estado,
    catalogo: Catalogo,
    filtro: Filtro,
    /// Índices de `catalogo.familias` que pasan el filtro y la búsqueda.
    visibles: Vec<usize>,
    seleccion: Option<Seleccion>,
    estilo: usize,
    ficha: EstadoFicha,
    /// Cada petición de ficha lleva su número: si llega una vieja después de
    /// cambiar de familia, se descarta.
    ficha_gen: usize,
    buscador: Entity<Entrada>,
    muestra: Entity<Entrada>,
    tamano: f32,
    pista_deslizador: Rc<Cell<Bounds<Pixels>>>,
    arrastrando: bool,
    lista: UniformListScrollHandle,
    detalle: ScrollHandle,
    dialogo: bool,
    toast: Option<(SharedString, usize)>,
    error: Option<ErrorPagina>,
    ocupado: bool,
    /// Archivos ya pasados a la base de datos de GPUI. Se registran una vez:
    /// GPUI no libera nunca una fuente añadida.
    registradas: HashSet<PathBuf>,
    pagina_gen: usize,
    toggle_gen: usize,
    foco: FocusHandle,
    _subs: Vec<Subscription>,
}

impl Fuentes {
    pub fn new(archivos: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let t = Textos::del_sistema();
        let buscador = cx.new(|cx| Entrada::new("", t.buscar, cx));
        let muestra = cx.new(|cx| Entrada::new(t.muestra, t.muestra_pista, cx));
        let subs = vec![
            cx.subscribe(&buscador, |this, _, _: &Cambio, cx| this.filtrar(cx)),
            cx.subscribe(&muestra, |_, _, _: &Cambio, cx| cx.notify()),
            cx.observe_window_appearance(window, |this, window, cx| this.recargar_tema(window, cx)),
            // Ajustes puede haber cambiado la paleta mientras la ventana no
            // tenía el foco: se relee al volver, como hacen las apps web.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.recargar_tema(window, cx);
                }
                cx.notify();
            }),
        ];
        let mut this = Fuentes {
            t,
            tema: Tema::cargar(es_oscuro(window)),
            estado: Estado::Cargando,
            catalogo: Catalogo::default(),
            filtro: Filtro::Todas,
            visibles: Vec::new(),
            seleccion: None,
            estilo: 0,
            ficha: EstadoFicha::Cargando,
            ficha_gen: 0,
            buscador,
            muestra,
            tamano: 48.,
            pista_deslizador: Rc::default(),
            arrastrando: false,
            lista: UniformListScrollHandle::new(),
            detalle: ScrollHandle::new(),
            dialogo: false,
            toast: None,
            error: None,
            ocupado: false,
            registradas: HashSet::new(),
            pagina_gen: 0,
            toggle_gen: 0,
            foco: cx.focus_handle(),
            _subs: subs,
        };
        this.colorear_entradas(cx);
        this.recargar(None, cx);
        if !archivos.is_empty() {
            this.abrir(archivos, cx);
        }
        this
    }

    fn recargar_tema(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.tema = Tema::cargar(es_oscuro(window));
        self.colorear_entradas(cx);
        cx.notify();
    }

    fn colorear_entradas(&mut self, cx: &mut Context<Self>) {
        let (pista, cursor) = (self.tema.tx2, self.tema.blue);
        for e in [&self.buscador, &self.muestra] {
            e.update(cx, |e, cx| {
                e.color_pista = pista;
                e.color_cursor = cursor;
                cx.notify();
            });
        }
    }

    // ── Datos ───────────────────────────────────────────────────────────

    fn recargar(&mut self, elegir: Option<SharedString>, cx: &mut Context<Self>) {
        // Con una lista ya en pantalla se recarga por debajo: poner el
        // esqueleto en cada instalación sería un parpadeo de medio segundo.
        if self.catalogo.familias.is_empty() {
            self.estado = Estado::Cargando;
        }
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async { catalogo::cargar() })
                .await;
            this.update(cx, |this, cx| {
                match r {
                    Ok(c) => {
                        this.catalogo = c;
                        this.estado = Estado::Listo;
                        this.filtrar(cx);
                        let nombre = elegir.or_else(|| match &this.seleccion {
                            Some(Seleccion::Instalada(n)) => Some(n.clone()),
                            _ => None,
                        });
                        match nombre.filter(|n| this.indice_de(n).is_some()) {
                            Some(n) => this.elegir(Seleccion::Instalada(n), cx),
                            None if matches!(this.seleccion, Some(Seleccion::Archivos { .. })) => {}
                            None => this.elegir_primera(cx),
                        }
                    }
                    Err(e) => this.estado = Estado::Error(e.mensaje(this.t).into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn indice_de(&self, nombre: &str) -> Option<usize> {
        self.catalogo
            .familias
            .iter()
            .position(|f| f.nombre.as_ref() == nombre)
    }

    fn filtrar(&mut self, cx: &mut Context<Self>) {
        let busca = self.buscador.read(cx).texto().trim().to_lowercase();
        let filtro = self.filtro;
        self.visibles = self
            .catalogo
            .familias
            .iter()
            .enumerate()
            .filter(|(_, f)| match filtro {
                Filtro::Todas => true,
                Filtro::Mias => f.origen == Origen::Usuario,
                Filtro::Sistema => f.origen == Origen::Sistema,
            })
            .filter(|(_, f)| busca.is_empty() || f.clave.contains(&busca))
            .map(|(i, _)| i)
            .collect();
        cx.notify();
    }

    fn familia(&self) -> Option<&Familia> {
        match self.seleccion.as_ref()? {
            Seleccion::Instalada(n) => self.indice_de(n).map(|i| &self.catalogo.familias[i]),
            Seleccion::Archivos { familias, .. } => familias.first(),
        }
    }

    fn estilo_actual(&self) -> Option<&Estilo> {
        let f = self.familia()?;
        f.estilos.get(self.estilo).or(f.estilos.first())
    }

    fn elegir_primera(&mut self, cx: &mut Context<Self>) {
        match self.visibles.first() {
            Some(&i) => self.elegir(
                Seleccion::Instalada(self.catalogo.familias[i].nombre.clone()),
                cx,
            ),
            None => {
                self.seleccion = None;
                cx.notify();
            }
        }
    }

    fn elegir(&mut self, sel: Seleccion, cx: &mut Context<Self>) {
        let misma = match (&self.seleccion, &sel) {
            (Some(Seleccion::Instalada(a)), Seleccion::Instalada(b)) => a == b,
            _ => false,
        };
        self.seleccion = Some(sel);
        if !misma {
            // La regular primero: es la que se espera ver al abrir una familia.
            self.estilo = self
                .familia()
                .and_then(|f| {
                    f.estilos
                        .iter()
                        .position(|e| !e.cursiva && (e.peso - 400.).abs() < 1.)
                })
                .unwrap_or(0);
            self.pagina_gen += 1;
            self.error = None;
            self.detalle.set_offset(point(px(0.), px(0.)));
            if let Some(Seleccion::Instalada(n)) = &self.seleccion
                && let Some(pos) = self
                    .indice_de(n)
                    .and_then(|i| self.visibles.iter().position(|&v| v == i))
            {
                self.lista.scroll_to_item(pos, ScrollStrategy::Nearest);
            }
        }
        self.pedir_ficha(cx);
        cx.notify();
    }

    fn elegir_estilo(&mut self, i: usize, cx: &mut Context<Self>) {
        self.estilo = i;
        self.pedir_ficha(cx);
        cx.notify();
    }

    fn pedir_ficha(&mut self, cx: &mut Context<Self>) {
        self.ficha_gen += 1;
        let gen_ = self.ficha_gen;
        let Some(estilo) = self.estilo_actual().cloned() else {
            return;
        };
        self.ficha = EstadoFicha::Cargando;
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { catalogo::ficha(&estilo) })
                .await;
            this.update(cx, |this, cx| {
                if this.ficha_gen != gen_ {
                    return;
                }
                this.ficha = match r {
                    Ok(f) => EstadoFicha::Lista(f),
                    Err(e) => EstadoFicha::Error(e.mensaje(this.t).into()),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── Operaciones ────────────────────────────────────────────────────

    /// Enseña unos archivos sin instalarlos.
    fn abrir(&mut self, archivos: Vec<PathBuf>, cx: &mut Context<Self>) {
        let archivos: Vec<PathBuf> = archivos
            .into_iter()
            .filter(|p| catalogo::es_archivo_de_fuente(p))
            .collect();
        if archivos.is_empty() {
            return;
        }
        let nuevos: Vec<PathBuf> = archivos
            .iter()
            .filter(|p| !self.registradas.contains(*p))
            .cloned()
            .collect();
        let reintento = Operacion::Abrir(archivos.clone());
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let mut familias = Vec::new();
                    for a in &archivos {
                        familias.extend(catalogo::leer_archivo(a)?);
                    }
                    // GPUI solo conoce lo que había al arrancar: para dibujar
                    // un archivo suelto hay que dárselo entero.
                    let datos = nuevos
                        .iter()
                        .map(|p| std::fs::read(p).map(|d| (p.clone(), d)))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| Fallo::Disco(e.to_string()))?;
                    Ok::<_, Fallo>((familias, archivos, datos))
                })
                .await;
            this.update(cx, |this, cx| match r {
                Ok((familias, archivos, datos)) => {
                    this.registrar(datos, cx);
                    this.elegir(Seleccion::Archivos { familias, archivos }, cx);
                    this.pagina_gen += 1;
                }
                Err(e) => this.fallar(e, Some(reintento), cx),
            })
            .ok();
        })
        .detach();
    }

    fn registrar(&mut self, datos: Vec<(PathBuf, Vec<u8>)>, cx: &mut Context<Self>) {
        let nuevos: Vec<Cow<'static, [u8]>> = datos
            .into_iter()
            .filter(|(p, _)| self.registradas.insert(p.clone()))
            .map(|(_, d)| Cow::Owned(d))
            .collect();
        if !nuevos.is_empty()
            && let Err(e) = cx.text_system().add_fonts(nuevos)
        {
            self.fallar(Fallo::NoEsFuente(e.to_string()), None, cx);
        }
    }

    fn instalar(&mut self, archivos: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.ocupado {
            return;
        }
        self.ocupado = true;
        self.error = None;
        cx.notify();
        let lista = archivos.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let datos = catalogo::instalar(&lista)?;
                    let primera = catalogo::leer_archivo(&lista[0])?
                        .into_iter()
                        .next()
                        .map(|f| f.nombre);
                    Ok::<_, Fallo>((lista.into_iter().zip(datos).collect::<Vec<_>>(), primera))
                })
                .await;
            this.update(cx, |this, cx| {
                this.ocupado = false;
                match r {
                    Ok((datos, primera)) => {
                        this.registrar(datos, cx);
                        if let Some(n) = &primera {
                            this.seleccion = Some(Seleccion::Instalada(n.clone()));
                        }
                        this.recargar(primera, cx);
                    }
                    Err(e) => this.fallar(e, Some(Operacion::Instalar(archivos)), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn activar(&mut self, activar: bool, cx: &mut Context<Self>) {
        let Some(familia) = self.familia().cloned() else {
            return;
        };
        self.toggle_gen += 1;
        self.ocupado = true;
        // La UI ya enseña el estado pedido: si falla, la recarga lo devuelve
        // al real y la tarjeta de error dice por qué.
        if let Some(i) = self.indice_de(&familia.nombre) {
            self.catalogo.familias[i].desactivada = !activar;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { catalogo::desactivar(&familia, !activar) })
                .await;
            this.update(cx, |this, cx| {
                this.ocupado = false;
                if let Err(e) = r {
                    this.fallar(e, Some(Operacion::Activar(activar)), cx);
                }
                this.recargar(None, cx);
            })
            .ok();
        })
        .detach();
    }

    fn eliminar(&mut self, cx: &mut Context<Self>) {
        self.dialogo = false;
        let Some(familia) = self.familia().cloned() else {
            return;
        };
        // La siguiente de la lista, para no dejar el detalle vacío.
        let siguiente = self
            .indice_de(&familia.nombre)
            .and_then(|i| self.visibles.iter().position(|&v| v == i))
            .and_then(|p| {
                self.visibles
                    .get(p + 1)
                    .or(p.checked_sub(1).and_then(|p| self.visibles.get(p)))
            })
            .map(|&i| self.catalogo.familias[i].nombre.clone());
        self.ocupado = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let nombre = familia.nombre.clone();
            let r = cx
                .background_executor()
                .spawn(async move { catalogo::eliminar(&familia) })
                .await;
            this.update(cx, |this, cx| {
                this.ocupado = false;
                match r {
                    Ok(()) => {
                        this.seleccion = None;
                        this.mostrar_toast(this.t.toast_eliminada.replace("{}", &nombre), cx);
                        this.recargar(siguiente, cx);
                    }
                    Err(e) => this.fallar(e, Some(Operacion::Eliminar), cx),
                }
            })
            .ok();
        })
        .detach();
    }

    fn reintentar(&mut self, op: Operacion, cx: &mut Context<Self>) {
        self.error = None;
        match op {
            Operacion::Instalar(a) => self.instalar(a, cx),
            Operacion::Activar(a) => self.activar(a, cx),
            Operacion::Eliminar => self.eliminar(cx),
            Operacion::Abrir(a) => self.abrir(a, cx),
        }
    }

    fn fallar(&mut self, e: Fallo, reintento: Option<Operacion>, cx: &mut Context<Self>) {
        self.error = Some(ErrorPagina {
            mensaje: e.mensaje(self.t).into(),
            reintento,
        });
        self.detalle.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    /// «Instalar fuentes…»: el explorador de BookOS como selector (§9).
    fn elegir_archivos(&mut self, cx: &mut Context<Self>) {
        let salida = std::env::temp_dir().join(format!(
            "bookos-fonts-pick-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let mut orden = std::process::Command::new("bookos-explorer");
        orden.arg("--bookos-portal").arg(&salida).args([
            "--pick-multiple",
            "--pick-filter-name",
            self.t.app,
        ]);
        for patron in ["*.ttf", "*.otf", "*.ttc", "*.otc"] {
            orden.args(["--pick-pattern", patron]);
        }
        orden.args([
            "--pick-title",
            self.t.instalar_fuentes.trim_end_matches('…'),
            "--pick-accept",
            self.t.instalar,
        ]);
        let hijo = match orden.spawn() {
            Ok(h) => h,
            // Sin el explorador instalado, el portal del sistema.
            Err(_) => return self.elegir_con_portal(cx),
        };
        cx.spawn(async move |this, cx| {
            let elegidos = cx
                .background_executor()
                .spawn(async move { esperar_selector(hijo, &salida) })
                .await;
            if let Some(archivos) = elegidos.filter(|a| !a.is_empty()) {
                this.update(cx, |this, cx| this.instalar(archivos, cx)).ok();
            }
        })
        .detach();
    }

    fn elegir_con_portal(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(self.t.instalar.into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(archivos))) = rx.await {
                this.update(cx, |this, cx| this.instalar(archivos, cx)).ok();
            }
        })
        .detach();
    }

    fn mostrar_en_archivos(&mut self, cx: &mut Context<Self>) {
        let Some(estilo) = self.estilo_actual() else {
            return;
        };
        if let Err(e) = std::process::Command::new("bookos-explorer")
            .arg(&estilo.archivo)
            .spawn()
        {
            self.error = Some(ErrorPagina {
                mensaje: self.t.err_explorador.replace("{}", &e.to_string()).into(),
                reintento: None,
            });
            cx.notify();
        }
    }

    fn mostrar_toast(&mut self, texto: String, cx: &mut Context<Self>) {
        let gen_ = self.toast.as_ref().map_or(0, |(_, g)| g + 1);
        self.toast = Some((texto.into(), gen_));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(VIDA_TOAST).await;
            this.update(cx, |this, cx| {
                if this.toast.as_ref().is_some_and(|(_, g)| *g == gen_) {
                    this.toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn mover_seleccion(&mut self, paso: isize, cx: &mut Context<Self>) {
        let actual = match &self.seleccion {
            Some(Seleccion::Instalada(n)) => self
                .indice_de(n)
                .and_then(|i| self.visibles.iter().position(|&v| v == i)),
            _ => None,
        };
        let destino = match actual {
            Some(p) => p
                .saturating_add_signed(paso)
                .min(self.visibles.len().saturating_sub(1)),
            None => 0,
        };
        if let Some(&i) = self.visibles.get(destino) {
            self.elegir(
                Seleccion::Instalada(self.catalogo.familias[i].nombre.clone()),
                cx,
            );
        }
    }

    fn al_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialogo {
            self.dialogo = false;
        } else if !self.buscador.read(cx).texto().is_empty() {
            self.buscador.update(cx, |b, cx| b.vaciar(cx));
        } else {
            window.focus(&self.foco, cx);
        }
        cx.notify();
    }

    fn mover_deslizador(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let b = self.pista_deslizador.get();
        if b.size.width <= px(0.) {
            return;
        }
        let f = ((x - b.left()) / b.size.width).clamp(0., 1.);
        self.tamano = (TAMANO_MIN + f * (TAMANO_MAX - TAMANO_MIN)).round();
        cx.notify();
    }
}

const TAMANO_MIN: f32 = 12.;
const TAMANO_MAX: f32 = 144.;

fn es_oscuro(window: &Window) -> bool {
    matches!(
        window.appearance(),
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
    )
}

/// Espera a que el explorador escriba la selección o se cierre sin elegir.
fn esperar_selector(mut hijo: std::process::Child, salida: &Path) -> Option<Vec<PathBuf>> {
    let resultado = loop {
        if let Ok(texto) = std::fs::read_to_string(salida) {
            break Some(
                texto
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(PathBuf::from)
                    .collect(),
            );
        }
        if hijo.try_wait().ok().flatten().is_some() {
            // Pudo escribir justo antes de salir.
            break std::fs::read_to_string(salida).ok().map(|t| {
                t.lines()
                    .filter(|l| !l.is_empty())
                    .map(PathBuf::from)
                    .collect()
            });
        }
        std::thread::sleep(Duration::from_millis(80));
    };
    std::fs::remove_file(salida).ok();
    resultado
}

/// `cubic-bezier()` de CSS, para las curvas de la guía.
fn bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 {
    move |t| {
        let curva = |a: f32, b: f32, s: f32| {
            3. * a * s * (1. - s).powi(2) + 3. * b * s * s * (1. - s) + s.powi(3)
        };
        // Se busca el `s` cuya x es `t` por bisección: 20 pasos dan un error
        // de 1e-6, de sobra para 60 fotogramas por segundo.
        let (mut lo, mut hi) = (0f32, 1f32);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.;
            if curva(x1, x2, mid) < t {
                lo = mid
            } else {
                hi = mid
            }
        }
        curva(y1, y2, (lo + hi) / 2.)
    }
}

/// Las licencias traen el texto entero de la OFL; con unos 220 caracteres
/// ya se sabe cuál es.
fn recortar(texto: SharedString, max: usize) -> SharedString {
    match texto.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", texto[..i].trim_end()).into(),
        None => texto,
    }
}

fn formato_tamano(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1 << 20) as f64),
        b => format!("{} kB", (b as f64 / 1024.).ceil()),
    }
}

// ── Pintado ─────────────────────────────────────────────────────────────

impl Render for Fuentes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = self.tema;
        let decoraciones = window.window_decorations();
        let (pegada, maximizada) = match decoraciones {
            Decorations::Server => (true, false),
            Decorations::Client { tiling } => (tiling.is_tiled(), window.is_maximized()),
        };
        let suelta = !pegada && !maximizada;
        window.set_client_inset(if suelta { BORDE } else { px(0.) });

        let ventana = div()
            .id("ventana")
            .size_full()
            .flex()
            .flex_col()
            .bg(tema.bg)
            .text_color(tema.tx)
            .font_family(FUENTE_UI)
            .text_size(px(14.))
            .overflow_hidden()
            .relative()
            .when(suelta, |d| {
                d.rounded(px(20.)).border_1().border_color(tema.brd)
            })
            .cursor(CursorStyle::Arrow)
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.arrastrando {
                    if e.pressed_button == Some(MouseButton::Left) {
                        this.mover_deslizador(e.position.x, cx);
                    } else {
                        this.arrastrando = false;
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.arrastrando = false),
            )
            .on_drop(cx.listener(|this, rutas: &ExternalPaths, _, cx| {
                this.abrir(rutas.paths().to_vec(), cx)
            }))
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.border_color(tema.blue))
            .child(self.barra(window))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.lateral(window, cx))
                    .child(self.detalle(window, cx)),
            )
            .children(
                self.toast
                    .as_ref()
                    .map(|(texto, g)| toast(&tema, texto.clone(), *g)),
            )
            .when(self.dialogo, |d| d.child(self.dialogo_eliminar(suelta, cx)));

        div()
            .id("raiz")
            .key_context("Fuentes")
            .track_focus(&self.foco)
            .on_action(cx.listener(Self::al_escape))
            .on_action(cx.listener(|this, _: &Buscar, window, cx| {
                let foco = this.buscador.read(cx).focus_handle(cx);
                window.focus(&foco, cx);
            }))
            .on_action(cx.listener(|_, _: &Cerrar, window, _| window.remove_window()))
            .size_full()
            .when(suelta, |d| d.p(BORDE).child(asas(window)))
            .child(ventana)
    }
}

/// Zonas invisibles alrededor de la ventana para redimensionarla: con la
/// decoración del lado del cliente el escritorio no las pone.
fn asas(window: &Window) -> impl IntoElement {
    let tamano = window.window_bounds().get_bounds().size;
    div()
        .id("asas")
        .absolute()
        .size_full()
        .child(
            canvas(
                move |bounds, window, _| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
                move |_, hitbox, window, _| {
                    if let Some(borde) = borde_en(window.mouse_position(), tamano) {
                        window.set_cursor_style(cursor_de(borde), &hitbox);
                    }
                },
            )
            .size_full(),
        )
        .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, _| {
            if let Some(borde) = borde_en(e.position, window.window_bounds().get_bounds().size) {
                window.start_window_resize(borde);
            }
        })
}

fn borde_en(p: gpui::Point<Pixels>, tamano: Size<Pixels>) -> Option<ResizeEdge> {
    // La esquina se agarra con algo más de margen que el lado: 8 px en
    // diagonal son difíciles de acertar.
    let esquina = BORDE * 2.;
    let (izq, der) = (p.x < BORDE, p.x > tamano.width - BORDE);
    let (arr, aba) = (p.y < BORDE, p.y > tamano.height - BORDE);
    let (izq_e, der_e) = (p.x < esquina, p.x > tamano.width - esquina);
    let (arr_e, aba_e) = (p.y < esquina, p.y > tamano.height - esquina);
    Some(match () {
        _ if (arr && izq_e) || (izq && arr_e) => ResizeEdge::TopLeft,
        _ if (arr && der_e) || (der && arr_e) => ResizeEdge::TopRight,
        _ if (aba && izq_e) || (izq && aba_e) => ResizeEdge::BottomLeft,
        _ if (aba && der_e) || (der && aba_e) => ResizeEdge::BottomRight,
        _ if arr => ResizeEdge::Top,
        _ if aba => ResizeEdge::Bottom,
        _ if izq => ResizeEdge::Left,
        _ if der => ResizeEdge::Right,
        _ => return None,
    })
}

fn cursor_de(borde: ResizeEdge) -> CursorStyle {
    match borde {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn icono(nombre: &'static str, tamano: f32, color: Hsla) -> gpui::Svg {
    svg()
        .path(format!("iconos/{nombre}.svg"))
        .size(px(tamano))
        .flex_none()
        .text_color(color)
}

fn tarjeta(tema: &Tema, radio: f32) -> gpui::Div {
    div()
        .bg(tema.card)
        .rounded(px(radio))
        .shadow(tema.sombra_reposo())
}

fn cabecera_seccion(tema: &Tema, texto: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(tema.tx2)
        .mt(px(18.))
        .mb(px(8.))
        .ml(px(6.))
        .child(texto.into())
}

/// Divisor de 0,5 px con 20 px de margen a cada lado, encima de la fila.
fn divisor(tema: &Tema, margen: f32) -> gpui::Div {
    div()
        .absolute()
        .top_0()
        .left(px(margen))
        .right(px(margen))
        .h(px(0.5))
        .bg(tema.div)
}

fn fila(tema: &Tema, primera: bool) -> gpui::Div {
    div()
        .relative()
        .min_h(px(48.))
        .px(px(20.))
        .py(px(14.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .when(!primera, |d| d.child(divisor(tema, 20.)))
}

fn textos_fila(
    tema: &Tema,
    titulo: impl Into<SharedString>,
    sub: Option<SharedString>,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .child(titulo.into()),
        )
        .children(sub.map(|s| {
            div()
                .text_size(px(11.))
                .text_color(tema.tx2)
                .mt(px(2.))
                .child(s)
        }))
}

fn toggle(
    tema: &Tema,
    id: &'static str,
    encendido: bool,
    gen_: usize,
    apagado: bool,
) -> gpui::Stateful<gpui::Div> {
    let tema = *tema;
    let fondo = if encendido { tema.blue } else { tema.toff };
    let (desde, hasta) = if encendido { (0., 18.) } else { (18., 0.) };
    let pulgar = div()
        .absolute()
        .top(px(3.))
        .size(px(22.))
        .rounded_full()
        .bg(gpui::white())
        .shadow(vec![tema::sombra(1., 3., 0.2)]);
    // Sin cambio del usuario no hay animación: abrir la página no puede
    // enseñar el interruptor moviéndose.
    let pulgar: AnyElement = if gen_ == 0 {
        pulgar.left(px(3. + hasta)).into_any_element()
    } else {
        let muelle = bezier(0.34, 1.4, 0.64, 1.);
        pulgar
            .with_animation(
                (id, gen_),
                // La curva va dentro del animador y no en `with_easing`: GPUI
                // exige que la curva no salga de [0, 1] (debug_assert) y el
                // muelle de la guía se pasa de 1 a propósito.
                Animation::new(DURACION_TOGGLE),
                // GPUI no tiene `transform`: se mueve con `left`, que aquí no
                // relayoutea nada más porque el pulgar va en absoluto.
                move |d, t| d.left(px(3. + desde + (hasta - desde) * muelle(t))),
            )
            .into_any_element()
    };
    div()
        .id(id)
        .relative()
        .flex_none()
        .w(px(46.))
        .h(px(28.))
        .rounded(px(14.))
        .bg(fondo)
        .when(apagado, |d| d.opacity(0.4))
        .when(!apagado, |d| d.cursor_pointer())
        .child(pulgar)
}

fn boton_secundario(
    tema: &Tema,
    id: &'static str,
    texto: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(18.))
        .py(px(8.))
        .rounded(px(12.))
        .bg(tema.hov)
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|s| s.bg(tema.act))
        .child(texto)
}

fn boton_primario(tema: &Tema, id: &'static str, texto: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(18.))
        .py(px(8.))
        .rounded(px(12.))
        .bg(tema.blue)
        .text_color(tema.on_accent)
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .child(texto)
}

fn toast(tema: &Tema, texto: SharedString, gen_: usize) -> impl IntoElement {
    div()
        .absolute()
        .bottom(px(28.))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(20.))
                .py(px(10.))
                .rounded(px(14.))
                .bg(tema.card)
                .border_1()
                .border_color(tema.brd)
                .shadow(vec![tema::sombra(4., 20., 0.15)])
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .child(icono("check-circle", 16., tema.tx2))
                .child(texto)
                .with_animation(("toast", gen_), Animation::new(DURACION_POPOVER), |d, t| {
                    d.opacity(t)
                }),
        )
}

impl Fuentes {
    fn barra(&self, window: &Window) -> impl IntoElement {
        let tema = self.tema;
        // Medidas de `bookos-shell/src/decoracion.rs` (§11.2): tinta al 55 %
        // con foco y al 30 % sin él; placa al 14/17 % solo al señalar.
        let tinta = tema.tx.opacity(if window.is_window_active() {
            0.55
        } else {
            0.30
        });
        let placa = tema.tx.opacity(if tema.oscuro { 0.17 } else { 0.14 });
        let boton = |id: &'static str, glifo: &'static str, cerrar: bool| {
            let grupo: SharedString = format!("g-{id}").into();
            div()
                .id(id)
                .group(grupo.clone())
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .map(|d| {
                    if cerrar {
                        d.hover(|s| s.bg(tema.red))
                    } else {
                        d.hover(|s| s.bg(placa))
                    }
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(icono(glifo, 16., tinta).group_hover(grupo, move |s| {
                    s.text_color(if cerrar { gpui::white() } else { tema.tx })
                }))
        };
        div()
            .id("barra")
            .h(px(36.))
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.))
            .pr(px(6.))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .on_click(|e: &ClickEvent, window, _| {
                if e.click_count() == 2 {
                    window.zoom_window();
                }
            })
            .child(
                boton("min", "ventana-minimizar", false)
                    .on_click(|_, window, _| window.minimize_window()),
            )
            .child(
                boton(
                    "max",
                    if window.is_maximized() {
                        "ventana-restaurar"
                    } else {
                        "ventana-maximizar"
                    },
                    false,
                )
                .on_click(|_, window, _| window.zoom_window()),
            )
            .child(
                boton("cerrar", "ventana-cerrar", true)
                    .on_click(|_, window, _| window.remove_window()),
            )
    }

    fn lateral(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = self.tema;
        let t = self.t;
        let hay_busqueda = !self.buscador.read(cx).texto().is_empty();
        let enfocado = self.buscador.read(cx).focus_handle(cx).is_focused(window);

        let buscador = div()
            .h(px(38.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .mb(px(14.))
            .rounded(px(14.))
            .bg(tema.sbg)
            .border_2()
            .border_color(if enfocado {
                tema.blue
            } else {
                gpui::transparent_black()
            })
            .text_color(tema.tx)
            .child(icono("magnifying-glass", 16., tema.tx2))
            .child(self.buscador.clone())
            .when(hay_busqueda, |d| {
                d.child(
                    div()
                        .id("limpiar")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.buscador.update(cx, |b, cx| b.vaciar(cx))
                        }))
                        .child(icono("x-mark", 16., tema.tx2)),
                )
            });

        let pestana = |id: &'static str, texto: &'static str, f: Filtro| {
            let activa = self.filtro == f;
            div()
                .id(id)
                .flex_1()
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.))
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if activa { tema.on_accent } else { tema.tx2 })
                .when(activa, |d| d.bg(tema.blue))
                .when(!activa, |d| {
                    d.cursor_pointer().hover(|s| s.text_color(tema.tx))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.filtro = f;
                    this.filtrar(cx);
                    this.lista.scroll_to_item(0, ScrollStrategy::Top);
                }))
                .child(texto)
        };
        let pestanas = div()
            .flex()
            .flex_none()
            .p(px(3.))
            .mb(px(18.))
            .rounded(px(12.))
            .bg(tema.sbg)
            .child(pestana("p-todas", t.pestana_todas, Filtro::Todas))
            .child(pestana("p-mias", t.pestana_mias, Filtro::Mias))
            .child(pestana("p-sistema", t.pestana_sistema, Filtro::Sistema));

        div()
            .w(px(384.))
            .flex_none()
            .flex()
            .flex_col()
            .px(px(24.))
            .pb(px(32.))
            .child(
                div()
                    .text_size(px(28.))
                    .font_weight(FontWeight::BOLD)
                    .mt(px(4.))
                    .mx(px(6.))
                    .mb(px(16.))
                    .child(t.app),
            )
            .child(buscador)
            .child(pestanas)
            .child(self.lista_familias(hay_busqueda, cx))
    }

    fn lista_familias(&self, hay_busqueda: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = self.tema;
        let t = self.t;
        let caja = tarjeta(&tema, 22.)
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col();
        match &self.estado {
            Estado::Cargando => {
                return caja.children((0..7).map(|i| {
                    div()
                        .relative()
                        .h(ALTO_FILA)
                        .px(px(16.))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .gap(px(6.))
                        .when(i > 0, |d| d.child(divisor(&tema, 16.)))
                        .child(
                            div()
                                .w(px(140. - (i % 3) as f32 * 24.))
                                .h(px(12.))
                                .rounded(px(6.))
                                .bg(tema.sbg),
                        )
                        .child(div().w(px(72.)).h(px(9.)).rounded(px(6.)).bg(tema.sbg))
                }));
            }
            Estado::Error(mensaje) => {
                return caja
                    .p(px(20.))
                    .gap(px(12.))
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(tema.tx2)
                            .text_center()
                            .child(mensaje.clone()),
                    )
                    .child(
                        boton_secundario(&tema, "reintentar-lista", t.reintentar)
                            .on_click(cx.listener(|this, _, _, cx| this.recargar(None, cx))),
                    );
            }
            Estado::Listo => {}
        }
        if self.visibles.is_empty() && hay_busqueda {
            return caja.p(px(28.)).items_center().child(
                div()
                    .text_size(px(13.))
                    .text_color(tema.tx2)
                    .text_center()
                    .child(t.sin_resultados),
            );
        }
        // La fila de «Instalar» va al final de la propia lista (§4). Con la
        // pestaña «Mías» vacía, esa fila es todo el contenido.
        let total = self.visibles.len() + 1;
        let lista = uniform_list(
            "familias",
            total,
            cx.processor(move |this, rango: std::ops::Range<usize>, _, cx| {
                rango.map(|i| this.fila_familia(i, cx)).collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.lista)
        .flex_1();
        caja.key_context("Lista")
            .on_action(cx.listener(|this, _: &Arriba, _, cx| this.mover_seleccion(-1, cx)))
            .on_action(cx.listener(|this, _: &Abajo, _, cx| this.mover_seleccion(1, cx)))
            .child(lista)
    }

    fn fila_familia(&self, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let tema = self.tema;
        let t = self.t;
        let base = div()
            .id(("familia", i))
            .relative()
            .w_full()
            .h(ALTO_FILA)
            .px(px(16.))
            .flex()
            .items_center()
            .gap(px(12.))
            .cursor_pointer()
            .hover(|s| s.bg(tema.hov))
            .active(|s| s.bg(tema.act))
            .when(i > 0, |d| d.child(divisor(&tema, 16.)))
            // El recorte de GPUI es rectangular: sin esto, el fondo de la
            // fila elegida asoma por las esquinas redondeadas de la tarjeta.
            .when(i == 0, |d| d.rounded_t(px(22.)))
            .when(i == self.visibles.len(), |d| d.rounded_b(px(22.)));

        let Some(&indice) = self.visibles.get(i) else {
            return base
                .child(
                    div()
                        .size(px(30.))
                        .flex_none()
                        .rounded_full()
                        .bg(tema.sbg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icono("plus", 16., tema.blue)),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(tema.blue)
                        .child(t.instalar_fuentes),
                )
                .on_click(cx.listener(|this, _, _, cx| this.elegir_archivos(cx)))
                .into_any_element();
        };
        let f = &self.catalogo.familias[indice];
        let elegida = matches!(&self.seleccion, Some(Seleccion::Instalada(n)) if *n == f.nombre);
        let mut sub = t.estilos(f.estilos.len());
        if f.desactivada {
            sub = format!("{sub} · {}", t.desactivada);
        } else if f.origen == Origen::Usuario && self.filtro != Filtro::Mias {
            sub = format!("{sub} · {}", t.origen_usuario);
        }
        let nombre = f.nombre.clone();
        // El nombre va en la letra de la interfaz, no en la suya: pintar cada
        // fila con su fuente obliga a GPUI a cargar cientos de archivos que
        // luego no suelta (la tabla de fuentes cargadas solo crece).
        base.when(elegida, |d| d.bg(tema.seleccion()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .when(f.desactivada, |d| d.text_color(tema.tx2))
                            .truncate()
                            .child(f.nombre.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(tema.tx2)
                            .mt(px(2.))
                            .truncate()
                            .child(sub),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.elegir(Seleccion::Instalada(nombre.clone()), cx);
                window.focus(&this.foco, cx);
            }))
            .into_any_element()
    }

    fn detalle(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = self.tema;
        let t = self.t;
        let caja = div()
            .id("detalle")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.detalle)
            .px(px(44.))
            .pb(px(32.));
        let Some(familia) = self.familia() else {
            // Mientras carga, el lateral ya enseña el esqueleto: aquí basta
            // con no pintar nada.
            if !matches!(self.estado, Estado::Listo) {
                return caja.into_any_element();
            }
            let texto = if self.filtro == Filtro::Mias && self.visibles.is_empty() {
                t.vacio_mias
            } else {
                t.vacio
            };
            return caja
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .max_w(px(360.))
                        .text_size(px(13.))
                        .text_color(tema.tx2)
                        .text_center()
                        .child(texto),
                )
                .into_any_element();
        };
        let estilo = self.estilo_actual().cloned();
        let archivos = match &self.seleccion {
            Some(Seleccion::Archivos { archivos, .. }) => Some(archivos.clone()),
            _ => None,
        };

        let mut sub = vec![t.estilos(familia.estilos.len())];
        sub.push(
            match familia.origen {
                Origen::Usuario => t.origen_usuario,
                Origen::Sistema => t.origen_sistema,
                Origen::Archivo => t.origen_archivo,
            }
            .to_string(),
        );
        if let Some(a) = archivos.as_ref().filter(|a| a.len() > 1) {
            sub.push(t.archivos_n.replace("{n}", &a.len().to_string()));
        }
        if familia.mono {
            sub.push(t.monoespaciada.to_string());
        }
        if familia.desactivada {
            sub.push(t.desactivada.to_string());
        }

        let cabecera = div()
            .pt(px(18.))
            .mb(px(20.))
            .flex()
            .items_end()
            .justify_between()
            .gap(px(16.))
            .child(
                div()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_weight(FontWeight::BOLD)
                            .truncate()
                            .child(familia.nombre.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(tema.tx2)
                            .mt(px(4.))
                            .child(sub.join(" · ")),
                    ),
            )
            .when_some(archivos.clone(), |d, archivos| {
                d.child(
                    boton_primario(
                        &tema,
                        "instalar",
                        if self.ocupado {
                            t.instalando
                        } else {
                            t.instalar
                        },
                    )
                    .when(self.ocupado, |b| b.opacity(0.6))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.instalar(archivos.clone(), cx)),
                    ),
                )
            });

        let contenido = div()
            .child(cabecera)
            .children(self.error.as_ref().map(|e| self.tarjeta_error(e, cx)))
            .child(self.tarjeta_vista(familia, estilo.as_ref(), window, cx))
            .child(cabecera_seccion(
                &tema,
                format!("{} · {}", t.seccion_estilos, familia.estilos.len()),
            ))
            .child(self.tarjeta_estilos(familia, cx))
            .children(self.tarjeta_caracteres(familia, estilo.as_ref()))
            .child(cabecera_seccion(&tema, t.seccion_info))
            .child(self.tarjeta_info(estilo.as_ref()))
            .child(cabecera_seccion(&tema, t.seccion_gestion))
            .child(self.tarjeta_gestion(familia, cx))
            // Fundido de entrada de página, sin desplazamiento (§2.7).
            .with_animation(
                ("pagina", self.pagina_gen),
                Animation::new(DURACION_PAGINA).with_easing(|t| 1. - (1. - t).powi(2)),
                |d, t| d.opacity(0.55 + 0.45 * t),
            );
        caja.child(contenido).into_any_element()
    }

    fn tarjeta_error(&self, e: &ErrorPagina, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = self.tema;
        tarjeta(&tema, 25.)
            .mb(px(16.))
            .px(px(20.))
            .py(px(14.))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(icono("exclamation-circle", 22., tema.red))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .child(e.mensaje.clone()),
            )
            .children(e.reintento.clone().map(|op| {
                boton_secundario(&tema, "reintentar", self.t.reintentar)
                    .on_click(cx.listener(move |this, _, _, cx| this.reintentar(op.clone(), cx)))
            }))
            .child(
                div()
                    .id("cerrar-error")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.error = None;
                        cx.notify();
                    }))
                    .child(icono("x-mark", 16., tema.tx2)),
            )
    }

    fn tarjeta_vista(
        &self,
        familia: &Familia,
        estilo: Option<&Estilo>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tema = self.tema;
        let t = self.t;
        let sin_latin = matches!(&self.ficha, EstadoFicha::Lista(f) if !f.tiene_latin);
        let texto = match self.muestra.read(cx).texto() {
            "" => t.muestra.to_string(),
            s => s.to_string(),
        };
        let campo = div()
            .flex_1()
            .min_w_0()
            .h(px(38.))
            .flex()
            .items_center()
            .px(px(12.))
            .rounded(px(14.))
            .bg(tema.sbg)
            .border_2()
            .border_color(
                if self.muestra.read(cx).focus_handle(cx).is_focused(window) {
                    tema.blue
                } else {
                    gpui::transparent_black()
                },
            )
            .child(self.muestra.clone());

        let pista = self.pista_deslizador.clone();
        let fraccion = (self.tamano - TAMANO_MIN) / (TAMANO_MAX - TAMANO_MIN);
        let deslizador = div()
            .id("deslizador")
            .w(px(180.))
            .h(px(28.))
            .flex_none()
            .relative()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    this.arrastrando = true;
                    this.mover_deslizador(e.position.x, cx);
                }),
            )
            .child(
                canvas(move |b, _, _| pista.set(b), |_, _, _, _| {})
                    .absolute()
                    .left(px(10.))
                    .right(px(10.))
                    .top(px(12.))
                    .h(px(4.)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.))
                    .right(px(10.))
                    .top(px(12.))
                    .h(px(4.))
                    .rounded(px(2.))
                    .bg(tema.toff),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.))
                    .top(px(12.))
                    .h(px(4.))
                    .w(px(160. * fraccion))
                    .rounded(px(2.))
                    .bg(tema.blue),
            )
            .child(
                div()
                    .absolute()
                    .top(px(4.))
                    .left(px(160. * fraccion))
                    .size(px(20.))
                    .rounded_full()
                    .bg(tema.blue)
                    .shadow(vec![tema::sombra(1., 3., 0.25)]),
            );

        let controles = div()
            .flex()
            .items_center()
            .gap(px(16.))
            .mb(px(20.))
            .child(campo)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(tema.tx2)
                    .child(t.tamano),
            )
            .child(deslizador)
            .child(
                div()
                    .w(px(40.))
                    .text_size(px(13.))
                    .text_color(tema.tx2)
                    .text_align(gpui::TextAlign::Right)
                    .child(format!("{} pt", self.tamano)),
            );

        let vista: AnyElement = if sin_latin {
            div()
                .py(px(28.))
                .text_size(px(13.))
                .text_color(tema.tx2)
                .text_center()
                .child(t.sin_latin)
                .into_any_element()
        } else {
            let peso = estilo.map_or(400., |e| e.peso);
            div()
                .font_family(familia.nombre.clone())
                .font_weight(FontWeight(peso))
                .when(estilo.is_some_and(|e| e.cursiva), |d| d.italic())
                .text_size(px(self.tamano))
                .line_height(gpui::relative(1.25))
                .overflow_hidden()
                .child(texto)
                .into_any_element()
        };

        tarjeta(&tema, 25.)
            .px(px(24.))
            .pt(px(20.))
            .pb(px(24.))
            .child(controles)
            .child(vista)
    }

    fn tarjeta_estilos(&self, familia: &Familia, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = self.tema;
        let t = self.t;
        let ultimo = familia.estilos.len().saturating_sub(1);
        tarjeta(&tema, 25.).children(familia.estilos.iter().enumerate().map(|(i, e)| {
            let detalle = if e.cursiva {
                format!("{} · {} · {}", t.peso(e.peso), e.peso.round(), t.cursiva)
            } else {
                format!("{} · {}", t.peso(e.peso), e.peso.round())
            };
            fila(&tema, i == 0)
                .id(("estilo", i))
                .cursor_pointer()
                .hover(|s| s.bg(tema.hov))
                .when(i == self.estilo, |d| d.bg(tema.seleccion()))
                .when(i == 0, |d| d.rounded_t(px(25.)))
                .when(i == ultimo, |d| d.rounded_b(px(25.)))
                .on_click(cx.listener(move |this, _, _, cx| this.elegir_estilo(i, cx)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(
                            div()
                                .font_family(familia.nombre.clone())
                                .font_weight(FontWeight(e.peso))
                                .when(e.cursiva, |d| d.italic())
                                .text_size(px(18.))
                                .truncate()
                                .child(e.nombre.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(tema.tx2)
                                .mt(px(2.))
                                .child(detalle),
                        ),
                )
        }))
    }

    fn tarjeta_caracteres(
        &self,
        familia: &Familia,
        estilo: Option<&Estilo>,
    ) -> Option<impl IntoElement> {
        let tema = self.tema;
        let cuerpo: AnyElement = match &self.ficha {
            EstadoFicha::Lista(f) if !f.tiene_latin || f.caracteres.is_empty() => return None,
            EstadoFicha::Error(_) => return None,
            EstadoFicha::Cargando => div()
                .h(px(120.))
                .rounded(px(14.))
                .bg(tema.sbg)
                .into_any_element(),
            EstadoFicha::Lista(f) => div()
                .flex()
                .flex_wrap()
                .gap(px(4.))
                .font_family(familia.nombre.clone())
                .font_weight(FontWeight(estilo.map_or(400., |e| e.peso)))
                .when(estilo.is_some_and(|e| e.cursiva), |d| d.italic())
                .text_size(px(22.))
                .children(f.caracteres.iter().map(|c| {
                    div()
                        .size(px(44.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.))
                        .hover(|s| s.bg(tema.hov))
                        .child(c.to_string())
                }))
                .into_any_element(),
        };
        Some(
            div()
                .child(cabecera_seccion(&tema, self.t.seccion_caracteres))
                .child(tarjeta(&tema, 25.).px(px(20.)).py(px(18.)).child(cuerpo)),
        )
    }

    fn tarjeta_info(&self, estilo: Option<&Estilo>) -> impl IntoElement {
        let tema = self.tema;
        let t = self.t;
        let raya = || SharedString::from("—");
        let ficha = match &self.ficha {
            EstadoFicha::Lista(f) => Some(f),
            _ => None,
        };
        let dato = |v: Option<&Option<String>>| -> SharedString {
            match v {
                Some(Some(s)) => s.clone().into(),
                _ => raya(),
            }
        };
        let mut filas: Vec<(&'static str, SharedString)> = vec![
            (
                t.info_estilo,
                estilo.map_or_else(raya, |e| e.nombre.clone()),
            ),
            (
                t.info_formato,
                ficha.map_or_else(raya, |f| f.formato.into()),
            ),
            (t.info_version, dato(ficha.map(|f| &f.version))),
            (t.info_disenador, dato(ficha.map(|f| &f.disenador))),
            (t.info_fabricante, dato(ficha.map(|f| &f.fabricante))),
            (
                t.info_glifos,
                ficha.map_or_else(raya, |f| f.glifos.to_string().into()),
            ),
        ];
        if let Some(f) = ficha.filter(|f| !f.ejes.is_empty()) {
            filas.push((t.info_ejes, f.ejes.join(", ").into()));
        }
        filas.push((
            t.info_tamano,
            ficha.map_or_else(raya, |f| formato_tamano(f.tamano).into()),
        ));
        filas.push((
            t.info_archivo,
            estilo.map_or_else(raya, |e| e.archivo.to_string_lossy().into_owned().into()),
        ));
        filas.push((t.info_copyright, dato(ficha.map(|f| &f.copyright))));
        filas.push((t.info_licencia, dato(ficha.map(|f| &f.licencia))));
        if let EstadoFicha::Error(e) = &self.ficha {
            filas.push(("", e.clone()));
        }

        tarjeta(&tema, 25.).children(filas.into_iter().enumerate().map(|(i, (titulo, valor))| {
            fila(&tema, i == 0)
                .items_start()
                .child(
                    div()
                        .flex_none()
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(titulo),
                )
                // Varias líneas antes que cortar: una ruta larga no cabe en una
                // y con puntos suspensivos se pierde el dato. `line_clamp` no
                // vale aquí: con el texto a la derecha GPUI enseña la última
                // línea en vez de la tercera.
                .child(
                    div()
                        .max_w(gpui::relative(0.68))
                        .text_size(px(13.))
                        .text_color(tema.tx2)
                        .text_align(gpui::TextAlign::Right)
                        .child(recortar(valor, 220)),
                )
        }))
    }

    fn tarjeta_gestion(&self, familia: &Familia, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = self.tema;
        let t = self.t;
        let instalada = familia.origen != Origen::Archivo;
        let protegida = self
            .catalogo
            .protegidas
            .iter()
            .any(|p| p == familia.nombre.as_ref());
        let mut tarjeta_ = tarjeta(&tema, 25.);
        let mut primera = true;
        if instalada {
            let encendida = !familia.desactivada;
            tarjeta_ = tarjeta_.child(
                fila(&tema, true)
                    .child(textos_fila(
                        &tema,
                        t.activada,
                        Some(
                            if protegida {
                                t.protegida_sub
                            } else {
                                t.activada_sub
                            }
                            .into(),
                        ),
                    ))
                    .child(
                        toggle(
                            &tema,
                            "t-activa",
                            encendida,
                            self.toggle_gen,
                            protegida || self.ocupado,
                        )
                        .when(!protegida, |d| {
                            d.on_click(cx.listener(move |this, _, _, cx| {
                                if !this.ocupado {
                                    this.activar(!encendida, cx);
                                }
                            }))
                        }),
                    ),
            );
            primera = false;
        }
        tarjeta_ = tarjeta_.child(
            fila(&tema, primera)
                .id("mostrar")
                .when(primera, |d| d.rounded_t(px(25.)))
                .when(familia.origen != Origen::Usuario, |d| d.rounded_b(px(25.)))
                .cursor_pointer()
                .hover(|s| s.bg(tema.hov))
                .on_click(cx.listener(|this, _, _, cx| this.mostrar_en_archivos(cx)))
                .child(textos_fila(&tema, t.mostrar_en_archivos, None))
                .child(icono("chevron-right", 16., tema.tx2)),
        );
        if familia.origen == Origen::Usuario {
            tarjeta_ =
                tarjeta_.child(
                    fila(&tema, false)
                        .child(textos_fila(&tema, t.eliminar, Some(t.eliminar_sub.into())))
                        // Secundario y no rojo: el rojo va en el diálogo (§4).
                        .child(boton_secundario(&tema, "eliminar", t.eliminar).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.dialogo = true;
                                cx.notify();
                            }),
                        )),
                );
        }
        tarjeta_
    }

    fn dialogo_eliminar(&self, suelta: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = self.tema;
        let t = self.t;
        let nombre = self
            .familia()
            .map(|f| f.nombre.to_string())
            .unwrap_or_default();
        let boton = |id: &'static str, texto: &'static str| {
            div()
                .id(id)
                .flex_1()
                .h(px(48.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(14.))
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .child(texto)
        };
        // Sin desenfoque detrás: GPUI no tiene `backdrop-filter`, y el scrim
        // al 45 % ya separa el diálogo de la página.
        div()
            .id("scrim")
            .absolute()
            .inset_0()
            .bg(gpui::black().opacity(0.45))
            // El recorte de la ventana es rectangular: sin su radio, el scrim
            // pintaría las esquinas que la ventana deja transparentes.
            .when(suelta, |d| d.rounded(px(20.)))
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, _, cx| {
                this.dialogo = false;
                cx.notify();
            }))
            .child(
                div()
                    .id("dialogo")
                    .w(px(310.))
                    .pt(px(28.))
                    .px(px(24.))
                    .pb(px(20.))
                    .rounded(px(26.))
                    .bg(tema.card)
                    .shadow(vec![tema::sombra(20., 56., 0.30)])
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::BOLD)
                            .text_center()
                            .line_height(gpui::relative(1.3))
                            .child(t.dialogo_eliminar_titulo.replace("{}", &nombre)),
                    )
                    .child(
                        div()
                            .mt(px(8.))
                            .text_size(px(13.))
                            .text_color(tema.tx2)
                            .text_center()
                            .line_height(gpui::relative(1.5))
                            .child(t.dialogo_eliminar_texto),
                    )
                    .child(
                        div()
                            .mt(px(20.))
                            .flex()
                            .gap(px(10.))
                            .child(
                                boton("d-cancelar", t.cancelar)
                                    .bg(tema.hov)
                                    .hover(|s| s.bg(tema.act))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dialogo = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                boton("d-eliminar", t.eliminar)
                                    .bg(tema.red)
                                    .text_color(gpui::white())
                                    .hover(|s| s.opacity(0.85))
                                    .on_click(cx.listener(|this, _, _, cx| this.eliminar(cx))),
                            ),
                    )
                    .with_animation("dialogo", Animation::new(DURACION_TOGGLE), |d, t| {
                        d.opacity(t)
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_curva_empieza_y_acaba_en_su_sitio() {
        let muelle = bezier(0.34, 1.4, 0.64, 1.);
        assert!(muelle(0.).abs() < 1e-3);
        assert!((muelle(1.) - 1.).abs() < 1e-3);
        // El muelle se pasa del final antes de volver.
        assert!((0..100).map(|i| muelle(i as f32 / 100.)).any(|v| v > 1.0));
    }

    #[test]
    fn las_esquinas_ganan_a_los_lados() {
        let t = gpui::size(px(800.), px(600.));
        assert!(matches!(
            borde_en(point(px(2.), px(2.)), t),
            Some(ResizeEdge::TopLeft)
        ));
        assert!(matches!(
            borde_en(point(px(2.), px(12.)), t),
            Some(ResizeEdge::TopLeft)
        ));
        assert!(matches!(
            borde_en(point(px(2.), px(300.)), t),
            Some(ResizeEdge::Left)
        ));
        assert!(matches!(
            borde_en(point(px(797.), px(599.)), t),
            Some(ResizeEdge::BottomRight)
        ));
        assert!(borde_en(point(px(400.), px(300.)), t).is_none());
    }

    #[test]
    fn recorta_por_caracteres_y_no_por_bytes() {
        assert_eq!(recortar("ñañaña".into(), 3).as_ref(), "ñañ…");
        assert_eq!(recortar("corto".into(), 10).as_ref(), "corto");
    }

    #[test]
    fn tamanos_de_archivo() {
        assert_eq!(formato_tamano(2048), "2 kB");
        assert_eq!(formato_tamano(3 << 20), "3.0 MB");
    }
}

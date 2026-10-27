//! Campo de texto de una línea.
//!
//! GPUI no trae ninguno: da el `EntityInputHandler`, que es lo que conecta con
//! el método de entrada (acentos, teclas muertas, compose), y lo demás lo pone
//! la app. Este es el ejemplo `input.rs` de GPUI recortado a lo que usan el
//! buscador y el texto de muestra.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString,
    Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div, fill, point, prelude::*,
    px, relative, size,
};

actions!(
    entrada,
    [
        Borrar,
        Suprimir,
        Izquierda,
        Derecha,
        SelIzquierda,
        SelDerecha,
        SelTodo,
        Inicio,
        Fin,
        Pegar,
        Cortar,
        Copiar
    ]
);

pub fn atajos() -> Vec<KeyBinding> {
    let c = Some("Entrada");
    vec![
        KeyBinding::new("backspace", Borrar, c),
        KeyBinding::new("delete", Suprimir, c),
        KeyBinding::new("left", Izquierda, c),
        KeyBinding::new("right", Derecha, c),
        KeyBinding::new("shift-left", SelIzquierda, c),
        KeyBinding::new("shift-right", SelDerecha, c),
        KeyBinding::new("secondary-a", SelTodo, c),
        KeyBinding::new("secondary-v", Pegar, c),
        KeyBinding::new("secondary-c", Copiar, c),
        KeyBinding::new("secondary-x", Cortar, c),
        KeyBinding::new("home", Inicio, c),
        KeyBinding::new("end", Fin, c),
    ]
}

/// El texto cambió; el dueño lo lee con [`Entrada::texto`].
pub struct Cambio;

pub struct Entrada {
    foco: FocusHandle,
    texto: SharedString,
    pista: SharedString,
    seleccion: Range<usize>,
    al_reves: bool,
    marcado: Option<Range<usize>>,
    linea: Option<ShapedLine>,
    limites: Option<Bounds<Pixels>>,
    /// Los colores los pone quien la pinta: cambian con la paleta.
    pub color_pista: Hsla,
    pub color_cursor: Hsla,
}

impl EventEmitter<Cambio> for Entrada {}

impl Entrada {
    pub fn new(
        texto: impl Into<SharedString>,
        pista: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        let texto = texto.into();
        Entrada {
            foco: cx.focus_handle(),
            seleccion: texto.len()..texto.len(),
            texto,
            pista: pista.into(),
            al_reves: false,
            marcado: None,
            linea: None,
            limites: None,
            color_pista: gpui::transparent_black(),
            color_cursor: gpui::transparent_black(),
        }
    }

    pub fn texto(&self) -> &str {
        &self.texto
    }

    pub fn vaciar(&mut self, cx: &mut Context<Self>) {
        self.cambiar("".into(), 0..0, cx);
    }

    fn cambiar(&mut self, texto: SharedString, seleccion: Range<usize>, cx: &mut Context<Self>) {
        let cambia = texto != self.texto;
        self.texto = texto;
        self.seleccion = seleccion;
        self.al_reves = false;
        if cambia {
            cx.emit(Cambio);
        }
        cx.notify();
    }

    fn cursor(&self) -> usize {
        if self.al_reves {
            self.seleccion.start
        } else {
            self.seleccion.end
        }
    }

    fn mover(&mut self, a: usize, cx: &mut Context<Self>) {
        self.seleccion = a..a;
        self.al_reves = false;
        cx.notify();
    }

    fn seleccionar_hasta(&mut self, a: usize, cx: &mut Context<Self>) {
        if self.al_reves {
            self.seleccion.start = a;
        } else {
            self.seleccion.end = a;
        }
        if self.seleccion.end < self.seleccion.start {
            self.al_reves = !self.al_reves;
            self.seleccion = self.seleccion.end..self.seleccion.start;
        }
        cx.notify();
    }

    // Límites por carácter, no por grafema: segmentar grafemas pide otra
    // dependencia y en un buscador de fuentes no se escriben emojis compuestos.
    fn anterior(&self, a: usize) -> usize {
        self.texto[..a]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn siguiente(&self, a: usize) -> usize {
        self.texto[a..]
            .chars()
            .next()
            .map_or(a, |c| a + c.len_utf8())
    }

    fn izquierda(&mut self, _: &Izquierda, _: &mut Window, cx: &mut Context<Self>) {
        let a = if self.seleccion.is_empty() {
            self.anterior(self.cursor())
        } else {
            self.seleccion.start
        };
        self.mover(a, cx);
    }

    fn derecha(&mut self, _: &Derecha, _: &mut Window, cx: &mut Context<Self>) {
        let a = if self.seleccion.is_empty() {
            self.siguiente(self.cursor())
        } else {
            self.seleccion.end
        };
        self.mover(a, cx);
    }

    fn sel_izquierda(&mut self, _: &SelIzquierda, _: &mut Window, cx: &mut Context<Self>) {
        self.seleccionar_hasta(self.anterior(self.cursor()), cx);
    }

    fn sel_derecha(&mut self, _: &SelDerecha, _: &mut Window, cx: &mut Context<Self>) {
        self.seleccionar_hasta(self.siguiente(self.cursor()), cx);
    }

    fn sel_todo(&mut self, _: &SelTodo, _: &mut Window, cx: &mut Context<Self>) {
        self.seleccion = 0..self.texto.len();
        self.al_reves = false;
        cx.notify();
    }

    fn inicio(&mut self, _: &Inicio, _: &mut Window, cx: &mut Context<Self>) {
        self.mover(0, cx);
    }

    fn fin(&mut self, _: &Fin, _: &mut Window, cx: &mut Context<Self>) {
        self.mover(self.texto.len(), cx);
    }

    fn borrar(&mut self, _: &Borrar, window: &mut Window, cx: &mut Context<Self>) {
        if self.seleccion.is_empty() {
            self.seleccionar_hasta(self.anterior(self.cursor()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn suprimir(&mut self, _: &Suprimir, window: &mut Window, cx: &mut Context<Self>) {
        if self.seleccion.is_empty() {
            self.seleccionar_hasta(self.siguiente(self.cursor()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn pegar(&mut self, _: &Pegar, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(texto) = cx.read_from_clipboard().and_then(|c| c.text()) {
            self.replace_text_in_range(None, &texto.replace('\n', " "), window, cx);
        }
    }

    fn copiar(&mut self, _: &Copiar, _: &mut Window, cx: &mut Context<Self>) {
        if !self.seleccion.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.texto[self.seleccion.clone()].to_string(),
            ));
        }
    }

    fn cortar(&mut self, _: &Cortar, window: &mut Window, cx: &mut Context<Self>) {
        self.copiar(&Copiar, window, cx);
        if !self.seleccion.is_empty() {
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn al_pulsar(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.foco, cx);
        let a = self.indice_en(e.position);
        if e.modifiers.shift {
            self.seleccionar_hasta(a, cx);
        } else {
            self.mover(a, cx);
        }
    }

    fn indice_en(&self, p: Point<Pixels>) -> usize {
        match (self.limites.as_ref(), self.linea.as_ref()) {
            (Some(b), Some(l)) if !self.texto.is_empty() => l.closest_index_for_x(p.x - b.left()),
            _ => 0,
        }
    }

    fn a_utf16(&self, a: usize) -> usize {
        self.texto[..a].chars().map(char::len_utf16).sum()
    }

    fn de_utf16(&self, a: usize) -> usize {
        let mut cuenta = 0;
        for (i, c) in self.texto.char_indices() {
            if cuenta >= a {
                return i;
            }
            cuenta += c.len_utf16();
        }
        self.texto.len()
    }

    fn rango_a_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.a_utf16(r.start)..self.a_utf16(r.end)
    }

    fn rango_de_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.de_utf16(r.start)..self.de_utf16(r.end)
    }
}

impl EntityInputHandler for Entrada {
    fn text_for_range(
        &mut self,
        rango: Range<usize>,
        real: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = self.rango_de_utf16(&rango);
        real.replace(self.rango_a_utf16(&r));
        Some(self.texto[r].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.rango_a_utf16(&self.seleccion),
            reversed: self.al_reves,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marcado.as_ref().map(|r| self.rango_a_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marcado = None;
    }

    fn replace_text_in_range(
        &mut self,
        rango: Option<Range<usize>>,
        nuevo: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = rango
            .map(|r| self.rango_de_utf16(&r))
            .or(self.marcado.clone())
            .unwrap_or(self.seleccion.clone());
        let texto = format!(
            "{}{}{}",
            &self.texto[..r.start],
            nuevo,
            &self.texto[r.end..]
        );
        let a = r.start + nuevo.len();
        self.marcado = None;
        self.cambiar(texto.into(), a..a, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        rango: Option<Range<usize>>,
        nuevo: &str,
        nueva_sel: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = rango
            .map(|r| self.rango_de_utf16(&r))
            .or(self.marcado.clone())
            .unwrap_or(self.seleccion.clone());
        let texto = format!(
            "{}{}{}",
            &self.texto[..r.start],
            nuevo,
            &self.texto[r.end..]
        );
        self.marcado = (!nuevo.is_empty()).then(|| r.start..r.start + nuevo.len());
        let sel = nueva_sel
            .map(|s| self.rango_de_utf16(&s))
            .map(|s| r.start + s.start..r.start + s.end)
            .unwrap_or(r.start + nuevo.len()..r.start + nuevo.len());
        self.cambiar(texto.into(), sel, cx);
    }

    fn bounds_for_range(
        &mut self,
        rango: Range<usize>,
        b: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let linea = self.linea.as_ref()?;
        let r = self.rango_de_utf16(&rango);
        Some(Bounds::from_corners(
            point(b.left() + linea.x_for_index(r.start), b.top()),
            point(b.left() + linea.x_for_index(r.end), b.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let local = self.limites?.localize(&p)?;
        let i = self.linea.as_ref()?.index_for_x(local.x)?;
        Some(self.a_utf16(i))
    }
}

impl Focusable for Entrada {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.foco.clone()
    }
}

impl Render for Entrada {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .key_context("Entrada")
            .track_focus(&self.foco)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::borrar))
            .on_action(cx.listener(Self::suprimir))
            .on_action(cx.listener(Self::izquierda))
            .on_action(cx.listener(Self::derecha))
            .on_action(cx.listener(Self::sel_izquierda))
            .on_action(cx.listener(Self::sel_derecha))
            .on_action(cx.listener(Self::sel_todo))
            .on_action(cx.listener(Self::inicio))
            .on_action(cx.listener(Self::fin))
            .on_action(cx.listener(Self::pegar))
            .on_action(cx.listener(Self::copiar))
            .on_action(cx.listener(Self::cortar))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::al_pulsar))
            .child(Texto {
                entrada: cx.entity(),
            })
    }
}

struct Texto {
    entrada: Entity<Entrada>,
}

struct Pintura {
    linea: ShapedLine,
    cursor: Option<PaintQuad>,
    seleccion: Option<PaintQuad>,
}

impl IntoElement for Texto {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Texto {
    type RequestLayoutState = ();
    type PrepaintState = Pintura;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        b: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Pintura {
        let e = self.entrada.read(cx);
        let estilo = window.text_style();
        let (mostrado, color) = if e.texto.is_empty() {
            (e.pista.clone(), e.color_pista)
        } else {
            (e.texto.clone(), estilo.color)
        };
        let base = TextRun {
            len: mostrado.len(),
            font: estilo.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match e.marcado.as_ref() {
            Some(m) => [
                TextRun {
                    len: m.start,
                    ..base.clone()
                },
                TextRun {
                    len: m.end - m.start,
                    underline: Some(UnderlineStyle {
                        color: Some(color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..base.clone()
                },
                TextRun {
                    len: mostrado.len() - m.end,
                    ..base
                },
            ]
            .into_iter()
            .filter(|r| r.len > 0)
            .collect(),
            None => vec![base],
        };
        let tamano = estilo.font_size.to_pixels(window.rem_size());
        let linea = window
            .text_system()
            .shape_line(mostrado, tamano, &runs, None);
        let (cursor, seleccion) = if e.seleccion.is_empty() {
            let x = if e.texto.is_empty() {
                px(0.)
            } else {
                linea.x_for_index(e.cursor())
            };
            (
                Some(fill(
                    Bounds::new(point(b.left() + x, b.top()), size(px(1.5), b.size.height)),
                    e.color_cursor,
                )),
                None,
            )
        } else {
            let rect = Bounds::from_corners(
                point(b.left() + linea.x_for_index(e.seleccion.start), b.top()),
                point(b.left() + linea.x_for_index(e.seleccion.end), b.bottom()),
            );
            (None, Some(fill(rect, e.color_cursor.opacity(0.25))))
        };
        Pintura {
            linea,
            cursor,
            seleccion,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        b: Bounds<Pixels>,
        _: &mut (),
        p: &mut Pintura,
        window: &mut Window,
        cx: &mut App,
    ) {
        let foco = self.entrada.read(cx).foco.clone();
        window.handle_input(&foco, ElementInputHandler::new(b, self.entrada.clone()), cx);
        if let Some(s) = p.seleccion.take() {
            window.paint_quad(s);
        }
        // Un fallo al pintar una línea ya formada solo puede venir del atlas de
        // glifos; se deja sin texto ese fotograma en vez de tumbar la app.
        p.linea
            .paint(
                b.origin,
                window.line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
        if foco.is_focused(window)
            && let Some(c) = p.cursor.take()
        {
            window.paint_quad(c);
        }
        let linea = p.linea.clone();
        self.entrada.update(cx, |e, _| {
            e.linea = Some(linea);
            e.limites = Some(b);
        });
    }
}

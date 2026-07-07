# Plugin Window Resizing in nih-plug + nih_plug_egui

A complete, working implementation for reusable plugin window resizing with
**discrete zoom buttons**, **sharp rendering at all sizes**, and **window
size persistence**. Targets nih-plug + nih_plug_egui (egui 0.31 era).

This documents the design decisions, the library bugs you have to patch
around, and the exact code. Tested working on macOS Retina (CLAP + VST3)
and on Windows VST3.

---

## Why this is hard

The naive expectation: call `GuiContext::request_resize()`, the host
resizes the window, your egui content fills it. Three layers conspire
against you:

1. **`nih_plug_egui::EguiState::set_requested_size` is private**, so you
   can't request a resize from your draw code without going through their
   `ResizableWindow` corner-drag widget (or patching the library).
2. **`nih_plug_egui::editor.rs` has a bug**: it clears `requested_size`
   *before* calling `request_resize()`, so the host queries `Editor::size()`
   and gets the OLD size — meaning the host approves a no-op resize.
3. **`nih_plug_egui::editor.rs` passes logical pixels into `PhySize`** (a
   type explicitly typed as physical pixels). On Retina, this halves the
   window when you try to grow it.
4. **`egui-baseview`'s renderer tessellates using its own private
   `pixels_per_point`** (see `renderer/opengl/renderer.rs:109`), NOT
   `egui_ctx.pixels_per_point()`. So if you call `set_pixels_per_point()`
   from your update closure to "lock" the canvas to a logical design size,
   egui's layout uses one ppp and the renderer uses another → blurry,
   misaligned output.

These library bugs mean **a clean implementation requires patching
`nih_plug_egui`** with a local fork in your project tree.

---

## High-level architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Source of truth: egui_state.size() — logical pixels         │
│ (persisted across sessions via #[persist] in Params)        │
└─────────────────────────────────────────────────────────────┘
                          ▲
       writes (request)   │   reads (each frame to compute scale)
                          │
┌─────────────────────────────────────────────────────────────┐
│ Header zoom buttons in GUI                                  │
│ — On click: egui_state.request_size(WINDOW_W * z,           │
│                                    WINDOW_H * z)            │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│ Patched editor.rs (per frame)                               │
│ — Pre-store new size into egui_state.size                   │
│ — Call context.request_resize(); host queries Editor::size()│
│ — On success: queue.resize(PhySize(w*native_ppp,            │
│                                    h*native_ppp))           │
│              send_viewport_cmd(InnerSize(w, h))             │
│ — On reject: revert egui_state.size, clear requested_size   │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│ Drawing code (each frame, in CentralPanel)                  │
│ — let scale = avail.width() / WINDOW_W                      │
│ — Multiply every fixed pixel value by `scale` via S(scale)  │
│ — DO NOT call set_pixels_per_point — leave ppp at native    │
└─────────────────────────────────────────────────────────────┘
```

The key insight: **content scaling is done in your drawing code via a
`scale` multiplier — NOT by manipulating egui's ppp.** Leave the native
ppp alone so egui's tessellation aligns with the framebuffer for crisp
rendering on Retina.

---

## File layout

```
your-plugin/
  Cargo.toml                              ← path override for nih_plug_egui
  patches/
    nih_plug_egui/
      Cargo.toml                          ← copy from upstream
      src/
        lib.rs                            ← + pub fn request_size()
        editor.rs                         ← bug fixes
        resizable_window.rs               ← unchanged (or remove)
        widgets.rs, widgets/*.rs          ← copy from upstream as-is
  src/
    gui.rs                                ← S struct + scaled draw fns
    params.rs                             ← #[persist = "editor"] EguiState
```

---

## Step 1 — Patch `nih_plug_egui`

Copy the entire `nih_plug_egui` directory from your local cargo cache
(`~/.cargo/git/checkouts/nih-plug-*/HASH/nih_plug_egui/`) into
`patches/nih_plug_egui/`.

In your project's `Cargo.toml`:

```toml
[dependencies]
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git" }
nih_plug_egui = { path = "patches/nih_plug_egui" }
```

In the patched `patches/nih_plug_egui/Cargo.toml`, change the nih_plug
dependency from a relative `path = ".."` to:

```toml
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", default-features = false }
```

(otherwise it tries to resolve a parent workspace that doesn't exist in
your project).

### Patch A — `lib.rs`: expose `set_requested_size` as `request_size`

```rust
impl EguiState {
    // ... existing methods ...

    fn set_requested_size(&self, new_size: (u32, u32)) {
        self.requested_size.store(Some(new_size));
    }

    /// Public API: request a logical-pixel resize. Library will pick this up
    /// next frame, call GuiContext::request_resize(), and (if host approves)
    /// resize the window. Safe to call every frame; only the last value wins.
    pub fn request_size(&self, width: u32, height: u32) {
        self.set_requested_size((width, height));
    }
}
```

### Patch B — `editor.rs`: fix resize-order + physical-pixel conversion

Replace the body of the `if let Some(new_size) = ...` block in the
`update` closure:

```rust
// If the window was requested to resize, handle it correctly.
// BUG FIX vs upstream: original code did `swap(None)` BEFORE calling
// `request_resize()`. But `request_resize()` makes the host call
// `Editor::size()`, which reads `requested_size` — so swapping to None
// first meant the host was told to resize to the OLD size.
// Fix: pre-store the new size into `egui_state.size` (which is the
// fallback when `requested_size` is None), then call request_resize.
if let Some(new_size) = egui_state.requested_size.load() {
    let old_size = egui_state.size.load();
    // Commit new size BEFORE asking the host. `Editor::size()` returns
    // `requested_size.or(size)`, so either way the host sees `new_size`.
    egui_state.size.store(new_size);

    if context.request_resize() {
        // Host approved. Clear request so we don't retry.
        egui_state.requested_size.store(None);

        // `new_size` is in LOGICAL pixels (macOS points / Win CSS px).
        // `Queue::resize` takes `PhySize` — explicitly PHYSICAL pixels —
        // so multiply by the OS-reported native scale factor.
        let native_ppp = egui_ctx.native_pixels_per_point().unwrap_or(1.0);
        let phy_w = (new_size.0 as f32 * native_ppp).round() as u32;
        let phy_h = (new_size.1 as f32 * native_ppp).round() as u32;

        queue.resize(PhySize::new(phy_w, phy_h));
        // This is what actually triggers the parented window to relayout
        // (see egui-baseview window.rs ViewportCommand::InnerSize handler).
        egui_ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
            new_size.0 as f32,
            new_size.1 as f32,
        )));
    } else {
        // Host rejected. Revert and clear request to avoid retry loops.
        egui_state.size.store(old_size);
        egui_state.requested_size.store(None);
    }
}
```

---

## Step 2 — Persist `EguiState` in your params

```rust
#[derive(Params)]
pub struct MyParams {
    #[persist = "editor"]
    pub editor_state: Arc<EguiState>,

    // ... your other params
}

impl Default for MyParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(WINDOW_W, WINDOW_H),
            // ...
        }
    }
}
```

In your plugin's `editor()` method:

```rust
fn editor(&mut self, _async: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
    gui::create_editor(
        self.params.clone(),
        self.params.editor_state.clone(),
        /* ...other args... */
    )
}
```

The window size is now automatically saved with each preset/session and
restored on reload.

---

## Step 3 — The `S` scale type

In `gui.rs`:

```rust
pub const WINDOW_W: u32 = 1080;
pub const WINDOW_H: u32 = 560;

/// Scale factor passed through all draw functions.
/// `S(1.0)` = native design size. `S(1.5)` = 150% zoom.
/// Multiply any fixed pixel value with `s.f(x)`, scale a Vec2 with `s.v(v)`,
/// build a scaled CornerRadius with `s.rr(r)`.
#[derive(Clone, Copy)]
pub struct S(pub f32);
impl S {
    #[inline] pub fn f(self, x: f32) -> f32 { x * self.0 }
    #[inline] pub fn v(self, v: Vec2) -> Vec2 { v * self.0 }
    #[inline] pub fn rr(self, r: f32) -> CornerRadius {
        let v = (r * self.0).round().clamp(0.0, 255.0) as u8;
        CornerRadius { nw: v, ne: v, sw: v, se: v }
    }
}
```

---

## Step 4 — Editor setup (NO `set_pixels_per_point`)

```rust
pub fn create_editor(
    params: Arc<MyParams>,
    egui_state: Arc<EguiState>,
    /* ...your shared state... */
) -> Option<Box<dyn Editor>> {
    let editor_state = std::sync::Mutex::new(EditorState::default());
    create_egui_editor(
        egui_state.clone(),
        editor_state,
        |_ctx, _| {},  // no setup needed
        move |egui_ctx, setter, state| {
            // CRITICAL: do NOT call egui_ctx.set_pixels_per_point() here.
            // egui-baseview's renderer tessellates using its own internal
            // ppp (from window scale events), so overriding egui_ctx's ppp
            // creates a mismatch that produces blurry/misaligned output.
            // Instead, leave ppp at native and scale all drawing by `S`.
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                let mut st = state.lock().unwrap();
                draw_plugin(ui, &params, setter, &mut st, egui_state.as_ref());
            });
        },
    )
}
```

---

## Step 5 — Top-level draw computes `S`

```rust
fn draw_plugin(ui: &mut Ui, params: &Arc<MyParams>, setter: &ParamSetter,
               st: &mut EditorState, egui_state: &EguiState) {
    let avail = ui.available_rect_before_wrap();
    // The single source of truth for content scale.
    let s = S(avail.width() / WINDOW_W as f32);

    // Scale all top-level layout dimensions.
    let header_rect = Rect::from_min_size(
        avail.min,
        Vec2::new(avail.width(), s.f(HEADER_H)),
    );
    draw_header(ui, header_rect, egui_state, s);

    let left_rect = Rect::from_min_max(
        body_rect.min,
        Pos2::new(body_rect.min.x + s.f(LEFT_W), body_rect.max.y),
    );
    // ... etc

    draw_left(ui, left_rect, params, setter, s);
    draw_center(ui, center_rect, params, setter, st, s);
    draw_right(ui, right_rect, params, setter, st, s);
}
```

Pass `s` through every draw function. Inside each, wrap every pixel
literal with `s.f(...)`:

```rust
// Before:
let knob_size = 50.0;
let label_offset_y = 30.0;
p.text(..., FontId::new(13.0, ...), ...);
p.rect_filled(rect, CornerRadius::same(4), ...);

// After:
let knob_size = s.f(50.0);
let label_offset_y = s.f(30.0);
p.text(..., FontId::new(s.f(13.0), ...), ...);
p.rect_filled(rect, s.rr(4.0), ...);
```

For widgets that take size parameters (like a `paint_knob(p, center, size, ...)`),
scale the size at the call site. Inside the widget, derive sub-dimensions
as fractions of `size` so they scale automatically:

```rust
fn paint_knob(p: &Painter, center: Pos2, size: f32, value: f32) {
    // `size` is already scaled by caller. Derive everything from it.
    let r_outer = size * 0.5 - size * 0.04;
    let stroke_w = (size * 0.05).max(1.0);
    // ...
}
```

---

## Step 6 — Zoom selector UI in the header

```rust
fn draw_header(ui: &mut Ui, rect: Rect, egui_state: &EguiState, s: S) {
    // ... your normal header stuff ...

    // Zoom selector
    let (cur_w, _) = egui_state.size();
    let cur_scale = cur_w as f32 / WINDOW_W as f32;
    let zooms: &[(f32, &str)] = &[
        (0.75, "75"), (1.00, "100"), (1.25, "125"),
        (1.50, "150"), (2.00, "200"),
    ];

    let btn_w = s.f(30.0);
    let btn_h = s.f(22.0);
    let gap = s.f(2.0);
    let total_w = btn_w * zooms.len() as f32 + gap * (zooms.len() as f32 - 1.0);
    let mut x = rect.max.x - s.f(12.0) - total_w;

    for (factor, label) in zooms.iter() {
        let r = Rect::from_min_size(
            Pos2::new(x, rect.center().y - btn_h * 0.5),
            Vec2::new(btn_w, btn_h),
        );
        let active = (cur_scale - factor).abs() < 0.01;
        let resp = ui.interact(r, ui.id().with(("zoom", label)), Sense::click());
        let bg = if active { c::ACCENT } else { c::BG_PLUGIN };
        let fg = if active { c::BG_PLUGIN } else { c::INK_DIM };

        let p = ui.painter();
        p.rect_filled(r, s.rr(4.0), bg);
        p.text(r.center(), Align2::CENTER_CENTER, *label,
               FontId::new(s.f(10.0), FontFamily::Proportional), fg);

        if resp.clicked() && !active {
            // The whole resize flow kicks off here.
            let target_w = (WINDOW_W as f32 * factor).round() as u32;
            let target_h = (WINDOW_H as f32 * factor).round() as u32;
            egui_state.request_size(target_w, target_h);
        }
        x += btn_w + gap;
    }
}
```

---

## How resize-on-click flows end-to-end

| Frame | What happens |
|-------|-------------|
| **0** | User clicks `150` button. `egui_state.request_size(1620, 840)` stores `Some((1620, 840))` in `requested_size`. |
| **1 (library)** | `editor.rs` sees `requested_size`, stores `(1620, 840)` into `egui_state.size`, calls `context.request_resize()`. Host calls `Editor::size()` → returns `(1620, 840)` (now correct because we pre-stored). Host approves. We `queue.resize(PhySize(3240, 1680))` on Retina (multiplied by `native_ppp=2.0`), and `send_viewport_cmd(InnerSize(1620, 840))`. |
| **2 (egui-baseview)** | Processes the `InnerSize` viewport command → calls `window.resize(Size{1620, 840})` → OS window grows. |
| **3 (your draw_plugin)** | `avail.width()` now returns 1620 (egui receives the new physical_size). `scale = 1620 / 1080 = 1.5`. All drawing uses `s.f(...)` → fills the larger window correctly. Sharp on Retina because egui_ctx's ppp = native ppp = renderer's ppp. |

---

## What does NOT work and why

These are approaches I tried and abandoned. Save yourself the time:

| Approach | Why it fails |
|----------|--------------|
| Dynamic `set_pixels_per_point` to lock canvas to 1080×560 points | egui-baseview's renderer tessellates with its own private ppp, not egui_ctx's. Mismatch → blurry, misaligned. |
| `ResizableWindow` corner drag with locked-canvas trick | Drag deltas in compressed coordinate space need conversion back to outer logical pixels; even when math is right, hosts handle per-frame resize spam poorly. |
| Continuous drag resize at all | Some DAWs (notably Ableton with VST3) just don't honor mid-session resize. Discrete buttons sidestep this — they're a single explicit user action. |
| Calling `request_resize` without pre-storing size | Library bug: clears `requested_size` before host queries `Editor::size()` → host gets OLD size → no-op resize. |
| Passing logical pixels into `PhySize` | `PhySize` is **physical** pixels by name and by source code. On Retina this halves your window. |

---

## Fact-checks for AI-generated advice

Hard-earned. If an AI tells you any of the following, it is wrong:

- **"`AtomicF32` exists in stable Rust"** — it doesn't. Use `AtomicU32` with `f32::to_bits()` / `f32::from_bits()`.
- **"`create_egui_editor`'s build closure returns the window size"** — it doesn't. Signature is `Fn(&Context, &mut T) -> ()`. Size lives in `EguiState`.
- **"`egui_state.size()` updates from layout calculations causing a feedback loop"** — it doesn't. It's an `AtomicCell.load()` of an explicitly-stored value. Only changes when the resize pipeline explicitly stores a new value.
- **"`GuiContext` has a `request_resize(w, h)` taking size args"** — it doesn't. `request_resize()` takes no args; the host queries the size via `Editor::size()`.
- **"CLAP synchronous re-entrancy crashes when calling `set_requested_size`"** — false for nih_plug_egui. `set_requested_size` is just an atomic store. The host call happens in the library's update loop next frame.
- **"VST3 wrappers report physical pixels back via `egui_state.size()`"** — false. The host has no way to push a size back; the size is whatever YOU stored. The `Editor` trait has a `size()` getter but no setter.

---

## Verified compatibility

- macOS 14 Retina: Logic CLAP ✓, Logic VST3 ✓, Bitwig CLAP ✓
- Windows 11: Reaper VST3 ✓, Bitwig CLAP ✓
- Render quality: sharp at 75%, 100%, 125%, 150%, 200% — same crispness as the un-resized plugin
- Window size persistence: saved/restored across DAW close+reopen

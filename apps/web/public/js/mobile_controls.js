/**
 * Controls overlay for Balls game.
 * Registers as a miniquad plugin. The FIRE button is shown on all devices;
 * joystick, weapon selector and zoom buttons are mobile-only.
 *
 * Controls added:
 *   - FIRE/USE button (bottom-right, all devices) → F key_down/up
 *       • Projectile weapons: hold = charge, release = fire
 *       • Airstrike / NapalmStrike: tap = TARGET at cursor position
 *       • Teleport: tap = USE (teleport to cursor position)
 *       • Build Wall: first tap = SET POS, second tap = SET ROT + place
 *       • Baseball Bat: tap = SWING
 *       • Placed weapons (Dynamite/Mine): tap = PLACE
 *     Button label updates automatically via js_set_fire_label callback from WASM.
 *   - Virtual joystick (bottom-left, mobile)  → Left / Right arrow key_down/up
 *                                               Push UP on joystick → Space (jump, one-shot)
 *   - Weapon button (bottom-right, mobile)    → Tab key_down  (toggles weapon menu)
 *   - Single-finger drag on canvas (mobile)   → click-drag pan (left_drag_panning, Rust 8 px threshold)
 *   - Single-finger tap on canvas (mobile)    → toggle aim-lock (or select weapon if menu open)
 *   - Two-finger drag on canvas (mobile)      → camera pan (left-button drag on midpoint)
 *   - Pinch on canvas (mobile)                → mouse_wheel (zoom)
 *   - Zoom + / − buttons (mobile)             → mouse_wheel
 */
(function () {
  "use strict";

  /* ── sapp key codes ── */
  var KEY_SPACE = 32;
  var KEY_TAB = 258;
  var KEY_LEFT = 263;
  var KEY_RIGHT = 262;
  var KEY_UP = 265;
  var KEY_F = 70; // F key — used by the FIRE button to start/release a charge

  /* Last canvas position the user aimed at (used by the FIRE button) */
  var lastAimX = 0;
  var lastAimY = 0;

  /* Track whether weapon menu is open so canvas drags scroll instead of aim */
  var menuOpen = false;

  function isTouchDevice() {
    return "ontouchstart" in window || navigator.maxTouchPoints > 0;
  }

  /* ── miniquad plugin hooks ── */
  function register_plugin() { /* Nothing to add to the WASM import object */ }

  function on_init() {
    // Show the fire button on all devices (desktop + mobile).
    // Touch-specific controls (joystick, weapon selector, zoom) are added only on touch devices.
    if (typeof wasm_exports === "undefined" || !wasm_exports.key_down) {
      var poll = setInterval(function () {
        if (typeof wasm_exports !== "undefined" && wasm_exports.key_down) {
          clearInterval(poll);
          initControls();
        }
      }, 200);
      return;
    }
    initControls();
  }

  /* ── Main initialisation ── */
  function initControls() {
    var canvas = document.querySelector("#glcanvas");
    if (!canvas) return;

    lastAimX = Math.floor(canvas.clientWidth / 2);
    lastAimY = Math.floor(canvas.clientHeight / 2);

    var touch = isTouchDevice();
    var fireBtn = buildOverlay(canvas, touch);

    // Register the label-update callback so ws_plugin.js can update the button text
    // whenever the Rust game state changes (weapon selected, mode entered, etc.).
    window.__updateFireLabel = function (label) {
      if (fireBtn) fireBtn.innerHTML = label.replace(/\n/g, "<br>");
    };

    if (touch) {
      setupCanvasTouches(canvas);
    }
  }

  /* ── Build the DOM overlay ── */
  function buildOverlay(canvas, touch) {
    var ov = document.createElement("div");
    ov.id = "mobile-controls-overlay";
    ov.style.cssText =
      "position:fixed;top:0;left:0;width:100%;height:100%;" +
      "pointer-events:none;z-index:999;" +
      "user-select:none;-webkit-user-select:none;touch-action:none;";
    document.body.appendChild(ov);

    if (touch) {
      /* ── Virtual joystick (bottom-left) ── */
      var js = buildJoystick();
      ov.appendChild(js.container);
      setupJoystick(js);

      /* ── Zoom +/− buttons (top-right, below the HUD) ── */
      var zoomInBtn = mkBtn("+", { top: "54px", right: "10px", w: "44px", h: "44px" });
      var zoomOutBtn = mkBtn("−", { top: "104px", right: "10px", w: "44px", h: "44px" });
      ov.appendChild(zoomInBtn);
      ov.appendChild(zoomOutBtn);

      zoomInBtn.addEventListener("touchstart", function (e) {
        e.preventDefault(); e.stopPropagation();
        wasm_exports.mouse_wheel(0, 60);
      }, false);
      zoomOutBtn.addEventListener("touchstart", function (e) {
        e.preventDefault(); e.stopPropagation();
        wasm_exports.mouse_wheel(0, -60);
      }, false);
    }

    /* ── WEAPON button – shown on ALL devices ── */
    var weaponBtnPos = touch
      ? { bottom: "150px", right: "100px", w: "80px", h: "60px" }
      : { bottom: "110px", right: "20px",  w: "80px", h: "60px" };
    var weaponBtn = mkBtn("🔫\nWEAPON", weaponBtnPos);
    ov.appendChild(weaponBtn);
    tapKeyAll(weaponBtn, KEY_TAB);
    /* Keep JS-side menuOpen flag in sync for both touch and mouse */
    weaponBtn.addEventListener("touchstart", function (e) { e.stopPropagation(); menuOpen = !menuOpen; }, false);
    weaponBtn.addEventListener("mousedown",  function (e) { e.stopPropagation(); menuOpen = !menuOpen; }, false);

    /* ── FIRE button – shown on ALL devices (mobile + desktop) ── */
    var fireBtnPos = touch
      ? { bottom: "80px", right: "10px", w: "80px", h: "130px", fontSize: "16px" }
      : { bottom: "20px", right: "20px", w: "80px", h: "80px", fontSize: "14px" };
    var fireBtn = mkBtn("🔥\nFIRE", Object.assign({
      bg: "rgba(120,25,15,0.90)", border: "rgba(230,80,60,0.95)",
    }, fireBtnPos));
    ov.appendChild(fireBtn);
    setupFireButton(fireBtn);

    return fireBtn; // returned so initControls can register the label-update callback
  }

  /* ── Virtual joystick DOM ── */
  function buildJoystick() {
    var container = document.createElement("div");
    container.style.cssText =
      "position:absolute;bottom:80px;left:10px;" +
      "width:130px;height:130px;pointer-events:auto;touch-action:none;";

    var base = document.createElement("div");
    base.style.cssText =
      "position:absolute;width:100%;height:100%;border-radius:50%;" +
      "background:rgba(255,255,255,0.10);border:2px solid rgba(255,255,255,0.30);" +
      "box-sizing:border-box;";

    var thumb = document.createElement("div");
    thumb.style.cssText =
      "position:absolute;width:52px;height:52px;border-radius:50%;" +
      "background:rgba(255,255,255,0.40);border:2px solid rgba(255,255,255,0.65);" +
      "top:50%;left:50%;transform:translate(-50%,-50%);" +
      "box-sizing:border-box;transition:transform 0.06s;";

    container.appendChild(base);
    container.appendChild(thumb);
    return { container: container, base: base, thumb: thumb };
  }

  function setupJoystick(js) {
    var active = false;
    var touchId = null;
    var center = { x: 0, y: 0 };
    var MAX_R = 42;
    /* held tracks which virtual keys are currently pressed */
    var held = { left: false, right: false, up: false };

    function setKey(side, code, on) {
      if (on === held[side]) return;
      held[side] = on;
      if (on) {
        wasm_exports.key_down(code, 0, false);
      } else {
        wasm_exports.key_up(code, 0);
      }
    }

    function reset() {
      active = false;
      touchId = null;
      js.thumb.style.transform = "translate(-50%,-50%)";
      setKey("left", KEY_LEFT, false);
      setKey("right", KEY_RIGHT, false);
      held.up = false;
    }

    js.container.addEventListener("touchstart", function (e) {
      e.preventDefault(); e.stopPropagation();
      if (active) return;
      active = true;
      touchId = e.changedTouches[0].identifier;
      var r = js.base.getBoundingClientRect();
      center.x = r.left + r.width / 2;
      center.y = r.top + r.height / 2;
    }, false);

    document.addEventListener("touchmove", function (e) {
      if (!active) return;
      for (var i = 0; i < e.changedTouches.length; i++) {
        var t = e.changedTouches[i];
        if (t.identifier !== touchId) continue;

        var dx = t.clientX - center.x;
        var dy = t.clientY - center.y;
        var dist = Math.sqrt(dx * dx + dy * dy);
        var clamped = Math.min(dist, MAX_R);
        var ang = Math.atan2(dy, dx);
        var tx = Math.cos(ang) * clamped;
        var ty = Math.sin(ang) * clamped;
        js.thumb.style.transform =
          "translate(calc(-50% + " + tx + "px),calc(-50% + " + ty + "px))";

        /* Normalised direction components (-1 … 1) */
        var nx = dist > 8 ? dx / dist : 0;
        var ny = dist > 8 ? dy / dist : 0;

        /* Left / right movement */
        setKey("left",  KEY_LEFT,  nx < -0.25);
        setKey("right", KEY_RIGHT, nx > 0.25);

        /* Up direction → jump (one-shot per gesture; resets when stick returns to neutral) */
        if (ny < -0.5 && !held.up) {
          held.up = true;
          wasm_exports.key_down(KEY_SPACE, 0, false);
          /* Brief press — macroquad only needs key_down → key_up transition to register is_key_pressed */
          setTimeout(function () { wasm_exports.key_up(KEY_SPACE, 0); }, 80);
        }
        /* Reset up-held when stick returns to roughly neutral/down so next push can re-jump */
        if (ny >= -0.25) {
          held.up = false;
        }
        break;
      }
    }, { passive: true });

    document.addEventListener("touchend", function (e) {
      for (var i = 0; i < e.changedTouches.length; i++) {
        if (e.changedTouches[i].identifier === touchId) { reset(); break; }
      }
    });
    document.addEventListener("touchcancel", function (e) {
      for (var i = 0; i < e.changedTouches.length; i++) {
        if (e.changedTouches[i].identifier === touchId) { reset(); break; }
      }
    });
  }

  /* ── FIRE button: hold = charge, release = fire.
   * Uses the F key so it never alters the WASM mouse position (no aim snap).
   * Works on both touch (touchstart/end) and desktop (mousedown/up/leave). ── */
  function setupFireButton(btn) {
    var held = false;
    function pressDown(e) {
      e.preventDefault(); e.stopPropagation();
      if (!held) {
        held = true;
        wasm_exports.key_down(KEY_F, 0, false);
      }
    }
    function pressUp(e) {
      e.preventDefault(); e.stopPropagation();
      if (held) {
        held = false;
        wasm_exports.key_up(KEY_F, 0);
      }
    }
    btn.addEventListener("touchstart",  pressDown, false);
    btn.addEventListener("touchend",    pressUp,   false);
    btn.addEventListener("touchcancel", pressUp,   false);
    btn.addEventListener("mousedown",   pressDown, false);
    btn.addEventListener("mouseup",     pressUp,   false);
    btn.addEventListener("mouseleave",  pressUp,   false);
  }

  /* ── Hold a key while the button is pressed ── */
  function holdKey(btn, code) {
    btn.addEventListener("touchstart", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.key_down(code, 0, false);
    }, false);
    btn.addEventListener("touchend", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.key_up(code, 0);
    }, false);
    btn.addEventListener("touchcancel", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.key_up(code, 0);
    }, false);
  }

  /* ── Tap a key once on press – touch + mouse (for toggle actions) ── */
  function tapKeyAll(btn, code) {
    function down(e) { e.preventDefault(); e.stopPropagation(); wasm_exports.key_down(code, 0, false); }
    function up(e)   { e.preventDefault(); e.stopPropagation(); wasm_exports.key_up(code, 0); }
    btn.addEventListener("touchstart",  down, false);
    btn.addEventListener("touchend",    up,   false);
    btn.addEventListener("touchcancel", up,   false);
    btn.addEventListener("mousedown",   down, false);
    btn.addEventListener("mouseup",     up,   false);
  }

  /* ── Tap a key once on press (for toggle actions) ── */
  function tapKey(btn, code) {
    btn.addEventListener("touchstart", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.key_down(code, 0, false);
    }, false);
    btn.addEventListener("touchend", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.key_up(code, 0);
    }, false);
    btn.addEventListener("touchcancel", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.key_up(code, 0);
    }, false);
  }

  /* Minimum pixel movement to register a pinch as intentional */
  var PINCH_THRESHOLD = 3;
  /* Multiplier converting pinch-distance-delta to scroll-wheel units */
  var PINCH_ZOOM_SENSITIVITY = 2.5;

  /* ── Canvas touch handlers ──────────────────────────────────────────────────
   *
   * IMPORTANT: All listeners use { capture: true, passive: false } and call
   * e.stopImmediatePropagation() to prevent gl.js's native canvas touch handlers
   * from mapping touches to mouse_down/mouse_up (which would fire the weapon).
   *
   * Single-finger: tap (< TAP_MOVE_THRESHOLD px) = aim-lock toggle / weapon select.
   *   Drag (any distance) = camera pan via the Rust left_drag_panning system —
   *   Rust automatically promotes a left-button press into camera pan once the
   *   cursor moves > 8 px, so no explicit PAN_THRESHOLD logic is needed here.
   *
   * Two-finger drag: camera pan (left-button drag on midpoint).
   * Two-finger pinch: zoom (mouse_wheel).
   */
  function setupCanvasTouches(canvas) {
    /* Single-finger state */
    var aimId = null;
    var aimStartX = 0, aimStartY = 0;
    var aimMoved = false;
    var TAP_MOVE_THRESHOLD = 15;
    /* Track whether we actually sent a mouse_down so we only send mouse_up to match. */
    var mouseDownSent = false;

    /* Two-finger pan / pinch state.
     * Uses left-button drag so it goes through the same left_drag_panning path
     * as desktop click-drag — eliminates the stuck-panning bug that occurred
     * when mouse_up fired off-canvas with the old right-button approach. */
    var panning = false;
    var lastPanCvs = null;
    var lastPinchDist = null;

    /* Menu-scroll state — tracked in CSS pixels (clientY) for device-independent
     * sensitivity regardless of the device pixel ratio. */
    var menuScrollClientY = null;
    var menuTouchStartClientX = null;
    var menuTouchStartClientY = null;
    var menuDragging = false;

    function cvsPos(clientX, clientY) {
      var r = canvas.getBoundingClientRect();
      var dpr = window.devicePixelRatio || 1;
      return {
        x: Math.floor((clientX - r.left) * dpr),
        y: Math.floor((clientY - r.top) * dpr),
      };
    }

    /* End a two-finger pan by releasing the left button. */
    function stopPan(pos) {
      if (panning) {
        wasm_exports.mouse_up(pos.x, pos.y, 0);
        panning = false;
        lastPanCvs = null;
        lastPinchDist = null;
      }
    }

    /* Cancel an in-progress single-finger touch without triggering a tap action.
     * Only sends mouse_up if mouse_down was previously sent (mouseDownSent). */
    function cancelSingleFinger(pos) {
      if (aimId !== null) {
        if (mouseDownSent) {
          wasm_exports.mouse_up(pos.x, pos.y, 0);
          mouseDownSent = false;
        }
        aimId = null;
        aimMoved = false;
      }
    }

    canvas.addEventListener("touchstart", function (e) {
      e.stopImmediatePropagation();
      e.preventDefault();

      var ts = e.touches;

      if (ts.length === 1) {
        /* Transitioning from two-finger back to one: stop pan cleanly */
        if (panning) stopPan(cvsPos(ts[0].clientX, ts[0].clientY));

        var p = cvsPos(ts[0].clientX, ts[0].clientY);
        aimId = ts[0].identifier;
        aimStartX = p.x; aimStartY = p.y;
        aimMoved = false;
        lastAimX = p.x; lastAimY = p.y;

        if (menuOpen) {
          /* Menu is open: set up scroll tracking using CSS pixels.
           * Do NOT send mouse_down — we defer the click to touchend so a scroll
           * gesture doesn't accidentally select a weapon on the first frame. */
          menuScrollClientY = ts[0].clientY;
          menuTouchStartClientX = ts[0].clientX;
          menuTouchStartClientY = ts[0].clientY;
          menuDragging = false;
          mouseDownSent = false;
        } else {
          /* Normal game interaction: snap aim and start a left-button press.
           * Rust will promote this to left_drag_panning once the finger moves > 8 px,
           * which covers both camera pan (any drag) and tap detection. */
          wasm_exports.mouse_move(p.x, p.y);
          wasm_exports.mouse_down(p.x, p.y, 0);
          mouseDownSent = true;
        }

      } else if (ts.length >= 2) {
        /* Two or more fingers: cancel single-finger and start two-finger pan */
        if (aimId !== null) {
          cancelSingleFinger({ x: lastAimX, y: lastAimY });
        }
        menuScrollClientY = null;
        var mid = midpoint(ts[0], ts[1]);
        var cMid = cvsPos(mid.x, mid.y);
        lastPinchDist = pinchDist(ts[0], ts[1]);
        if (!panning) {
          panning = true;
          wasm_exports.mouse_down(cMid.x, cMid.y, 0);
          lastPanCvs = cMid;
        }
      }
    }, { capture: true, passive: false });

    canvas.addEventListener("touchmove", function (e) {
      e.stopImmediatePropagation();
      e.preventDefault();

      var ts = e.touches;

      if (ts.length === 1 && aimId !== null) {
        var ct = e.changedTouches;
        for (var i = 0; i < ct.length; i++) {
          if (ct[i].identifier !== aimId) continue;
          var p = cvsPos(ct[i].clientX, ct[i].clientY);
          var totalMove = Math.abs(p.x - aimStartX) + Math.abs(p.y - aimStartY);
          if (totalMove > TAP_MOVE_THRESHOLD) aimMoved = true;
          lastAimX = p.x; lastAimY = p.y;

          if (menuOpen) {
            /* Scroll the weapon menu using CSS-pixel delta for device-independent
             * sensitivity. Factor ~0.3 gives roughly one weapon item per 15-20 px
             * of finger movement (item_h≈50 CSS px, Rust smooth factor = item_h/3). */
            var menuTotalMoveClient = Math.abs(ct[i].clientX - menuTouchStartClientX) +
                                   Math.abs(ct[i].clientY - menuTouchStartClientY);
            if (menuTotalMoveClient > TAP_MOVE_THRESHOLD) menuDragging = true;
            if (menuScrollClientY !== null) {
              var scrollDeltaClient = menuScrollClientY - ct[i].clientY;
              if (Math.abs(scrollDeltaClient) > 0.5) {
                wasm_exports.mouse_wheel(0, -scrollDeltaClient * 0.3);
              }
            }
            menuScrollClientY = ct[i].clientY;
            break;
          }

          /* Send mouse_move so aim tracks short drags; Rust auto-promotes to
           * left_drag_panning once the drag exceeds the 8 px threshold. */
          wasm_exports.mouse_move(p.x, p.y);
          break;
        }
      } else if (ts.length >= 2 && panning) {
        var mid = midpoint(ts[0], ts[1]);
        var cMid = cvsPos(mid.x, mid.y);
        wasm_exports.mouse_move(cMid.x, cMid.y);
        lastPanCvs = cMid;

        var d = pinchDist(ts[0], ts[1]);
        if (lastPinchDist !== null && Math.abs(d - lastPinchDist) > PINCH_THRESHOLD) {
          var delta = (d - lastPinchDist) * PINCH_ZOOM_SENSITIVITY;
          wasm_exports.mouse_wheel(0, delta);
          lastPinchDist = d;
        }
      }
    }, { capture: true, passive: false });

    canvas.addEventListener("touchend", function (e) {
      e.stopImmediatePropagation();
      e.preventDefault();

      var remaining = e.touches.length;
      /* Stop two-finger pan when fewer than 2 fingers remain */
      if (remaining < 2) {
        stopPan(lastPanCvs || { x: lastAimX, y: lastAimY });
      }
      if (remaining === 0) {
        if (aimId !== null) {
          if (menuOpen && !menuDragging) {
            /* Tap on the weapon menu: fire a click so Rust selects the weapon
             * (or closes the menu if tapped outside).  We send mouse_move first
             * so Rust has the correct cursor position, then down+up so
             * is_mouse_button_pressed fires on the next game frame. */
            wasm_exports.mouse_move(lastAimX, lastAimY);
            wasm_exports.mouse_down(lastAimX, lastAimY, 0);
            /* Release on the next animation frame so macroquad sees a full
             * press→release cycle with is_mouse_button_pressed = true. */
            (function (x, y) {
              requestAnimationFrame(function () {
                wasm_exports.mouse_up(x, y, 0);
              });
            }(lastAimX, lastAimY));
            menuOpen = false;
          } else if (mouseDownSent) {
            /* Normal game touch ended: release the button. */
            wasm_exports.mouse_up(lastAimX, lastAimY, 0);
          }
          /* menuOpen && menuDragging: scroll completed, no click — nothing to release
           * since no mouse_down was sent. */
          aimId = null;
          aimMoved = false;
          mouseDownSent = false;
          menuScrollClientY = null;
        }
      }
    }, { capture: true, passive: false });

    canvas.addEventListener("touchcancel", function (e) {
      e.stopImmediatePropagation();
      stopPan({ x: lastAimX, y: lastAimY });
      cancelSingleFinger({ x: lastAimX, y: lastAimY });
      menuScrollClientY = null;
      mouseDownSent = false;
    }, { capture: true, passive: false });
  }

  function midpoint(t1, t2) {
    return { x: (t1.clientX + t2.clientX) / 2, y: (t1.clientY + t2.clientY) / 2 };
  }
  function pinchDist(t1, t2) {
    var dx = t2.clientX - t1.clientX;
    var dy = t2.clientY - t1.clientY;
    return Math.sqrt(dx * dx + dy * dy);
  }

  /* ── Button factory ── */
  function mkBtn(html, opts) {
    var el = document.createElement("div");
    el.innerHTML = html.replace(/\n/g, "<br>");
    var s = [
      "position:absolute",
      "width:" + (opts.w || "64px"),
      "height:" + (opts.h || "54px"),
      "border-radius:10px",
      "background:" + (opts.bg || "rgba(25,45,75,0.88)"),
      "border:2px solid " + (opts.border || "rgba(70,120,190,0.90)"),
      "color:#fff",
      "font-size:" + (opts.fontSize || "12px"),
      "font-weight:bold",
      "display:flex",
      "flex-direction:column",
      "align-items:center",
      "justify-content:center",
      "text-align:center",
      "line-height:1.25",
      "pointer-events:auto",
      "-webkit-tap-highlight-color:transparent",
      "touch-action:none",
      "box-shadow:0 2px 6px rgba(0,0,0,0.5)",
    ];
    if (opts.bottom !== undefined) s.push("bottom:" + opts.bottom);
    if (opts.top    !== undefined) s.push("top:"    + opts.top);
    if (opts.left   !== undefined) s.push("left:"   + opts.left);
    if (opts.right  !== undefined) s.push("right:"  + opts.right);
    el.style.cssText = s.join(";");
    return el;
  }

  /* ── Register as a miniquad plugin ── */
  if (typeof miniquad_add_plugin !== "undefined") {
    miniquad_add_plugin({
      register_plugin: register_plugin,
      on_init: on_init,
      name: "mobile_controls",
      version: 1,
    });
  }
})();

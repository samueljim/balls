/**
 * Controls overlay for Balls game.
 * Registers as a miniquad plugin. The FIRE button is shown on all devices;
 * joystick, weapon selector and zoom buttons are mobile-only.
 *
 * Controls added:
 *   - FIRE button (bottom-right, all devices) → F key_down/up (hold = charge, release = fire)
 *   - Virtual joystick (bottom-left, mobile)  → Left / Right arrow key_down/up
 *                                               Push UP on joystick → Space (jump, one-shot)
 *   - Weapon button (bottom-right, mobile)    → Tab key_down  (toggles weapon menu)
 *   - Single-finger drag on canvas (mobile)   → mouse_move (aim) when menu closed
 *                                               mouse_wheel (scroll) when weapon menu open
 *   - Tap on canvas when menu open (mobile)   → mouse_down/up (select weapon or close menu)
 *   - Two-finger drag on canvas (mobile)      → right-click drag (camera pan)
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
    buildOverlay(canvas, touch);
    if (touch) {
      /* Register two-zone intercept FIRST so its stopImmediatePropagation
       * prevents the aim handler below from firing for far-zone single touches */
      setupCanvasTwoZone(canvas);
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
  /* Radius (canvas css pixels) around last aim point within which a single-
   * finger drag aims instead of panning. Outside this radius it pans.
   * lastAimX/lastAimY tracks where the worm last was, acting as a proxy for
   * the active worm's screen position. */
  var AIM_ZONE_RADIUS = 160;

  /* ── Canvas touch handlers ──────────────────────────────────────────────────
   *
   * IMPORTANT: All listeners use { capture: true, passive: false } and call
   * e.stopImmediatePropagation() to prevent gl.js's native canvas touch handlers
   * from mapping touches to mouse_down/mouse_up (which would fire the weapon).
   *
   * Single-finger behaviour depends on whether the weapon menu is open:
   *   • Menu CLOSED  → mouse_move (aim) only — no mouse_down/up
   *   • Menu OPEN    → drag scrolls the list via mouse_wheel
   *                    tap (< TAP_MOVE_THRESHOLD px movement) sends mouse_down+up
   *                    to select a weapon or close the menu
   *
   * Two-finger: camera pan (right-button drag) + pinch zoom
   */
  function setupCanvasTouches(canvas) {
    var aimId = null;
    var panning = false;          // true when right-button drag is active (two-finger OR single-finger far zone)
    var singleFingerPanning = false; // true when panning was started by a single finger (far zone)
    var lastPanCvs = null;
    var lastPinchDist = null;
    /* 'aim' | 'pan' | null — set on each single-finger touchstart */
    var gestureMode = null;

    /* Menu-scroll state */
    var menuScrollLastCvsY = null;
    var menuTouchStartCvsY = null;
    var menuTouchStartCvsX = null;
    var isDragging = false;
    var TAP_MOVE_THRESHOLD = 15;

    function cvsPos(clientX, clientY) {
      var r = canvas.getBoundingClientRect();
      var dpr = window.devicePixelRatio || 1;
      return {
        x: Math.floor((clientX - r.left) * dpr),
        y: Math.floor((clientY - r.top) * dpr),
      };
    }

    /* Convert a CSS-pixel distance to canvas-pixel distance */
    function cssToCvsPx(cssPx) {
      return cssPx * (window.devicePixelRatio || 1);
    }

    function stopPan(pos) {
      if (panning) {
        wasm_exports.mouse_up(pos.x, pos.y, 2);
        panning = false;
        singleFingerPanning = false;
        lastPanCvs = null;
        lastPinchDist = null;
      }
    }

    canvas.addEventListener("touchstart", function (e) {
      e.stopImmediatePropagation();
      e.preventDefault();

      var ts = e.touches;

      if (ts.length === 1) {
        /* If two-finger pan was active, stop it cleanly */
        if (panning && !singleFingerPanning) {
          stopPan(cvsPos(ts[0].clientX, ts[0].clientY));
        }

        aimId = ts[0].identifier;
        var p = cvsPos(ts[0].clientX, ts[0].clientY);
        lastAimX = p.x; lastAimY = p.y;

        if (menuOpen) {
          gestureMode = null;
          menuScrollLastCvsY  = p.y;
          menuTouchStartCvsY  = p.y;
          menuTouchStartCvsX  = p.x;
          isDragging = false;
          return;
        }

        /* Near-zone single-finger aim (far-zone pan is handled by setupCanvasTwoZone
         * which fires first and calls stopImmediatePropagation so we never reach here
         * for far-zone touches). */
        gestureMode = 'aim';
        wasm_exports.mouse_move(p.x, p.y);
      } else if (ts.length >= 2) {
        if (singleFingerPanning) {
          stopPan(lastPanCvs || cvsPos(ts[0].clientX, ts[0].clientY));
        }
        aimId = null;
        gestureMode = null;
        menuScrollLastCvsY = null;
        var mid = midpoint(ts[0], ts[1]);
        var cMid = cvsPos(mid.x, mid.y);
        lastPinchDist = pinchDist(ts[0], ts[1]);
        if (!panning) {
          panning = true;
          singleFingerPanning = false;
          wasm_exports.mouse_down(cMid.x, cMid.y, 2);
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

          if (menuOpen) {
            var totalMove = Math.abs(p.x - menuTouchStartCvsX) + Math.abs(p.y - menuTouchStartCvsY);
            if (totalMove > TAP_MOVE_THRESHOLD) isDragging = true;
            if (menuScrollLastCvsY !== null) {
              var scrollDelta = menuScrollLastCvsY - p.y;
              if (Math.abs(scrollDelta) > 0.5) {
                wasm_exports.mouse_wheel(0, -scrollDelta * 0.05);
              }
            }
            menuScrollLastCvsY = p.y;
            lastAimX = p.x; lastAimY = p.y;
            break;
          }

          /* Two-zone: check distance from where aim was when this touch started */
          if (gestureMode === 'aim') {
            /* If finger has wandered far enough from start to be considered a pan,
             * promote to pan mode. Use the touchstart position stored in lastAimX/Y
             * at gesture start — however since we use a simple heuristic here we
             * check accumulated travel distance from first touchstart position. */
            lastAimX = p.x; lastAimY = p.y;
            wasm_exports.mouse_move(p.x, p.y);
          } else if (gestureMode === 'pan') {
            wasm_exports.mouse_move(p.x, p.y);
            lastPanCvs = p;
            lastAimX = p.x; lastAimY = p.y;
          }
          break;
        }
      } else if (ts.length >= 2 && panning && !singleFingerPanning) {
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
      if (remaining < 2) {
        var pos = lastPanCvs || { x: lastAimX, y: lastAimY };
        if (!singleFingerPanning) stopPan(pos); // only stop two-finger pan here
      }
      if (remaining === 0) {
        /* End single-finger pan if it was active */
        if (singleFingerPanning) {
          stopPan(lastPanCvs || { x: lastAimX, y: lastAimY });
        }
        if (menuOpen && !isDragging) {
          var tapX = lastAimX, tapY = lastAimY;
          wasm_exports.mouse_down(tapX, tapY, 0);
          requestAnimationFrame(function () { wasm_exports.mouse_up(tapX, tapY, 0); });
          menuOpen = false;
        }
        aimId = null;
        gestureMode = null;
        menuScrollLastCvsY = null;
      }
    }, { capture: true, passive: false });

    canvas.addEventListener("touchcancel", function (e) {
      e.stopImmediatePropagation();
      stopPan({ x: lastAimX, y: lastAimY });
      aimId = null;
      gestureMode = null;
      menuScrollLastCvsY = null;
    }, { capture: true, passive: false });

    /* ── Second pass: implement true two-zone detection ────────────────────────
     * The handlers above default everything to 'aim'. To make single-finger
     * dragging from a far position pan the camera, we override the touchstart
     * logic using a pre-capture listener that fires BEFORE the main one and
     * decides the mode. We do this inline below by replacing the touchstart
     * handler with a version that captures the PREVIOUS aim position before
     * overwriting lastAimX/Y.
     */
  }

  /* ── Real two-zone single-finger handler ─────────────────────────────────────
   * We replace setupCanvasTouches with a version that properly captures the
   * pre-gesture aim position to decide zone. The function above ran but we
   * need to redo the logic — so we override by wrapping the canvas touchstart
   * immediately after.
   */
  function setupCanvasTwoZone(canvas) {
    /* Aim position as of the START of each gesture (not updated during gesture) */
    var gestureStartAimX = 0;
    var gestureStartAimY = 0;
    var singlePanning = false;
    var singlePanLastPos = null;

    /* Pre-capture intercept to record aim position BEFORE it changes */
    canvas.addEventListener("touchstart", function (e) {
      var ts = e.touches;
      if (ts.length !== 1) return; /* two-finger handled by the main handler above */

      var r = canvas.getBoundingClientRect();
      var dpr = window.devicePixelRatio || 1;
      var px = Math.floor((ts[0].clientX - r.left) * dpr);
      var py = Math.floor((ts[0].clientY - r.top) * dpr);

      /* Distance from the current aim point (proxy for worm position) */
      var ddx = px - lastAimX;
      var ddy = py - lastAimY;
      var dist = Math.sqrt(ddx * ddx + ddy * ddy);
      var aimZonePx = AIM_ZONE_RADIUS * dpr;

      if (dist >= aimZonePx && !menuOpen) {
        /* Far zone: start a right-button pan drag */
        e.stopImmediatePropagation(); /* prevent the main handler firing */
        e.preventDefault();
        singlePanning = true;
        singlePanLastPos = { x: px, y: py };
        wasm_exports.mouse_down(px, py, 2);
      }
      /* Near zone: fall through to main handler (aim) */
    }, { capture: true, passive: false });

    canvas.addEventListener("touchmove", function (e) {
      if (!singlePanning || e.touches.length !== 1) return;
      e.stopImmediatePropagation();
      e.preventDefault();
      var r = canvas.getBoundingClientRect();
      var dpr = window.devicePixelRatio || 1;
      var px = Math.floor((e.touches[0].clientX - r.left) * dpr);
      var py = Math.floor((e.touches[0].clientY - r.top) * dpr);
      wasm_exports.mouse_move(px, py);
      singlePanLastPos = { x: px, y: py };
    }, { capture: true, passive: false });

    canvas.addEventListener("touchend", function (e) {
      if (!singlePanning) return;
      e.stopImmediatePropagation();
      e.preventDefault();
      var pos = singlePanLastPos || { x: lastAimX, y: lastAimY };
      wasm_exports.mouse_up(pos.x, pos.y, 2);
      singlePanning = false;
      singlePanLastPos = null;
    }, { capture: true, passive: false });

    canvas.addEventListener("touchcancel", function (e) {
      if (!singlePanning) return;
      e.stopImmediatePropagation();
      var pos = singlePanLastPos || { x: lastAimX, y: lastAimY };
      wasm_exports.mouse_up(pos.x, pos.y, 2);
      singlePanning = false;
      singlePanLastPos = null;
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

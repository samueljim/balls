/**
 * Mobile touch controls overlay for Balls game.
 * Registers as a miniquad plugin; runs only on touch devices.
 *
 * Controls added:
 *   - Virtual joystick (bottom-left)  → Left / Right arrow key_down/up
 *   - Jump button (above joystick)    → Space key_down
 *   - Weapon button (bottom-right)    → Tab key_down  (toggles weapon menu)
 *   - End Turn button                 → E key_down
 *   - Zoom + / − buttons              → mouse_wheel
 *   - Single-finger drag on canvas    → mouse_move  (updates aim angle)
 *   - Two-finger drag on canvas       → right-click drag  (camera pan)
 *   - Pinch on canvas                 → mouse_wheel  (zoom)
 *   - FIRE button (bottom-right)      → mouse_down / mouse_up at last aim pos
 */
(function () {
  "use strict";

  /* ── sapp key codes (same as into_sapp_keycode in gl.js) ── */
  var KEY_SPACE = 32;
  var KEY_E = 69;
  var KEY_TAB = 258;
  var KEY_LEFT = 263;
  var KEY_RIGHT = 262;
  var KEY_UP = 265;
  var KEY_DOWN = 264;

  /* Last canvas position the user aimed at (used by the FIRE button) */
  var lastAimX = 0;
  var lastAimY = 0;

  function isTouchDevice() {
    return "ontouchstart" in window || navigator.maxTouchPoints > 0;
  }

  /* ── miniquad plugin hooks ── */
  function register_plugin() {
    /* Nothing to add to the WASM import object */
  }

  function on_init() {
    if (!isTouchDevice()) return;
    /* wasm_exports is available at this point (set before init_plugins is called) */
    if (typeof wasm_exports === "undefined" || !wasm_exports.key_down) {
      /* Fallback poll in case timing is off */
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

    /* Seed aim position to canvas centre */
    lastAimX = Math.floor(canvas.clientWidth / 2);
    lastAimY = Math.floor(canvas.clientHeight / 2);

    buildOverlay(canvas);
    setupCanvasTouches(canvas);
  }

  /* ── Build the DOM overlay ── */
  function buildOverlay(canvas) {
    var ov = document.createElement("div");
    ov.id = "mobile-controls-overlay";
    ov.style.cssText =
      "position:fixed;top:0;left:0;width:100%;height:100%;" +
      "pointer-events:none;z-index:999;" +
      "user-select:none;-webkit-user-select:none;touch-action:none;";
    document.body.appendChild(ov);

    /* ── Virtual joystick (bottom-left) ── */
    var js = buildJoystick();
    ov.appendChild(js.container);
    setupJoystick(js);

    /* ── Jump button (sits above the joystick) ── */
    var jumpBtn = mkBtn("▲\nJUMP", {
      bottom: "230px", left: "45px", w: "60px", h: "50px",
    });
    ov.appendChild(jumpBtn);
    holdKey(jumpBtn, KEY_SPACE);

    /* ── Weapon menu button (bottom-right) ── */
    var weaponBtn = mkBtn("🔫\nWEAPON", {
      bottom: "150px", right: "100px", w: "80px", h: "60px",
    });
    ov.appendChild(weaponBtn);
    tapKey(weaponBtn, KEY_TAB);

    /* ── End Turn button ── */
    var endBtn = mkBtn("✓\nEND TURN", {
      bottom: "80px", right: "100px", w: "80px", h: "60px",
      bg: "rgba(30,80,40,0.88)", border: "rgba(60,160,70,0.9)",
    });
    ov.appendChild(endBtn);
    tapKey(endBtn, KEY_E);

    /* ── FIRE button (big, bottom-right) ── */
    var fireBtn = mkBtn("🔥\nFIRE", {
      bottom: "80px", right: "10px", w: "80px", h: "130px",
      bg: "rgba(120,25,15,0.90)", border: "rgba(230,80,60,0.95)",
      fontSize: "16px",
    });
    ov.appendChild(fireBtn);
    setupFireButton(fireBtn);

    /* ── Zoom +/− buttons (top-right, below the HUD) ── */
    var zoomInBtn = mkBtn("+", {
      top: "54px", right: "10px", w: "44px", h: "44px",
    });
    var zoomOutBtn = mkBtn("−", {
      top: "104px", right: "10px", w: "44px", h: "44px",
    });
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
    /* MAX_R: max drag radius in CSS px — roughly 65% of the 130px joystick base radius */
    var MAX_R = 42;
    var held = { left: false, right: false };

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

    /* touchmove/end/cancel are on `document` intentionally so the joystick
     * stays responsive when the finger moves outside the joystick bounds. */
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

        var nx = dist > 8 ? dx / dist : 0;
        setKey("left", KEY_LEFT, nx < -0.25);
        setKey("right", KEY_RIGHT, nx > 0.25);
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

  /* ── FIRE button: hold = charge, release = fire ── */
  function setupFireButton(btn) {
    btn.addEventListener("touchstart", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.mouse_down(lastAimX, lastAimY, 0);
    }, false);
    btn.addEventListener("touchend", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.mouse_up(lastAimX, lastAimY, 0);
    }, false);
    btn.addEventListener("touchcancel", function (e) {
      e.preventDefault(); e.stopPropagation();
      wasm_exports.mouse_up(lastAimX, lastAimY, 0);
    }, false);
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
  var PINCH_ZOOM_SENSITIVITY = 1.8;

  /* ── Canvas touch handlers for aiming + camera pan/zoom ── */
  function setupCanvasTouches(canvas) {
    var aimId = null;
    var panning = false;
    var lastPanCvs = null;
    var lastPinchDist = null;

    function cvsPos(clientX, clientY) {
      var r = canvas.getBoundingClientRect();
      var dpr = window.devicePixelRatio || 1;
      return {
        x: Math.floor((clientX - r.left) * dpr),
        y: Math.floor((clientY - r.top) * dpr),
      };
    }

    function stopPan(pos) {
      if (panning) {
        wasm_exports.mouse_up(pos.x, pos.y, 2);
        panning = false;
        lastPanCvs = null;
        lastPinchDist = null;
      }
    }

    canvas.addEventListener("touchstart", function (e) {
      var ts = e.touches;
      if (ts.length === 1) {
        /* Single finger: aim */
        stopPan(cvsPos(ts[0].clientX, ts[0].clientY));
        aimId = ts[0].identifier;
        var p = cvsPos(ts[0].clientX, ts[0].clientY);
        lastAimX = p.x; lastAimY = p.y;
        wasm_exports.mouse_move(p.x, p.y);
      } else if (ts.length >= 2) {
        /* Two fingers: camera pan + pinch zoom */
        aimId = null;
        var mid = midpoint(ts[0], ts[1]);
        var cMid = cvsPos(mid.x, mid.y);
        lastPinchDist = pinchDist(ts[0], ts[1]);
        if (!panning) {
          panning = true;
          wasm_exports.mouse_down(cMid.x, cMid.y, 2);
          lastPanCvs = cMid;
        }
      }
    }, { capture: true, passive: true });

    canvas.addEventListener("touchmove", function (e) {
      var ts = e.touches;
      if (ts.length === 1 && aimId !== null) {
        var ct = e.changedTouches;
        for (var i = 0; i < ct.length; i++) {
          if (ct[i].identifier === aimId) {
            var p = cvsPos(ct[i].clientX, ct[i].clientY);
            lastAimX = p.x; lastAimY = p.y;
            wasm_exports.mouse_move(p.x, p.y);
            break;
          }
        }
      } else if (ts.length >= 2 && panning) {
        var mid = midpoint(ts[0], ts[1]);
        var cMid = cvsPos(mid.x, mid.y);
        wasm_exports.mouse_move(cMid.x, cMid.y);
        lastPanCvs = cMid;

        /* Pinch zoom */
        var d = pinchDist(ts[0], ts[1]);
        if (lastPinchDist !== null && Math.abs(d - lastPinchDist) > PINCH_THRESHOLD) {
          var delta = (d - lastPinchDist) * PINCH_ZOOM_SENSITIVITY;
          wasm_exports.mouse_wheel(0, delta);
          lastPinchDist = d;
        }
      }
    }, { capture: true, passive: true });

    canvas.addEventListener("touchend", function (e) {
      var remaining = e.touches.length;
      if (remaining < 2) {
        var pos = lastPanCvs || { x: lastAimX, y: lastAimY };
        stopPan(pos);
      }
      if (remaining === 0) aimId = null;
    }, { capture: true, passive: true });

    canvas.addEventListener("touchcancel", function () {
      stopPan({ x: lastAimX, y: lastAimY });
      aimId = null;
    }, { capture: true, passive: true });
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

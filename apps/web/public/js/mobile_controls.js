/**
 * Mobile touch controls overlay for Balls game.
 * Registers as a miniquad plugin; runs only on touch devices.
 *
 * Controls added:
 *   - Virtual joystick (bottom-left)  → set_analog_walk(nx) for proportional speed
 *                                       Push UP on joystick → Space (jump, one-shot)
 *   - JUMP button (right of joystick) → Space key_down/up (reliable tap)
 *   - Weapon button (bottom-right)    → Tab key_down  (toggles weapon menu)
 *   - FIRE button (bottom-right)      → mouse_down / mouse_up at last aim pos
 *   - Single-finger drag on canvas    → mouse_move (aim ONLY — never fires)
 *                                       mouse_wheel (scroll) when weapon menu open
 *   - Tap on canvas when menu open    → mouse_down/up (select weapon or close menu)
 *   - Two-finger drag on canvas       → right-click drag (camera pan)
 *   - Pinch on canvas                 → mouse_wheel (zoom)
 *   - Zoom + / − buttons              → mouse_wheel
 *
 * Key design principle: canvas single-finger touch ONLY aims (mouse_move).
 * The FIRE button is the ONLY path that sends mouse_down(left) for weapon charging.
 * While FIRE is held (fireButtonDown), canvas mouse_move is suppressed to prevent
 * the drag-to-pan guard (left_drag_panning) in Rust from cancelling the charge.
 */
(function () {
  "use strict";

  /* ── sapp key codes ── */
  var KEY_SPACE = 32;
  var KEY_TAB = 258;
  var KEY_LEFT = 263;  /* kept for fallback */
  var KEY_RIGHT = 262; /* kept for fallback */

  /* Duration of a key-pulse in ms (key_down → key_up for one-shot actions like jump) */
  var KEY_PULSE_MS = 80;

  /* Joystick analog remapping constants */
  var JOY_DEAD_ZONE = 0.25;       /* below this |nx|, no movement */
  var JOY_MIN_WALK = 0.5;         /* minimum output speed factor past the dead zone */
  var JOY_WALK_RANGE = 0.5;       /* range from MIN to 1.0 over the rest of the stick travel */

  /* Last canvas position the user aimed at (used by the FIRE button) */
  var lastAimX = 0;
  var lastAimY = 0;

  /* Track whether weapon menu is open so canvas drags scroll instead of aim */
  var menuOpen = false;

  /**
   * True while the FIRE button is physically held down.
   * When set:
   *  - canvas mouse_move events are NOT forwarded to WASM (prevents mouse_position()
   *    from drifting and triggering left_drag_panning which would cancel the charge)
   *  - canvas menu-tap mouse_down(0) is suppressed (avoid simultaneous accidental click)
   */
  var fireButtonDown = false;

  function isTouchDevice() {
    return "ontouchstart" in window || navigator.maxTouchPoints > 0;
  }

  /* ── miniquad plugin hooks ── */
  function register_plugin() { /* Nothing to add to the WASM import object */ }

  function on_init() {
    if (!isTouchDevice()) return;
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

    buildOverlay(canvas);
    /* Register two-zone intercept FIRST so its stopImmediatePropagation
     * prevents the aim handler below from firing for far-zone single touches */
    setupCanvasTwoZone(canvas);
    setupCanvasTouches(canvas);
  }

  /* ── Pulse a key (for one-shot actions like jump) ── */
  function pulseKey(code) {
    wasm_exports.key_down(code, 0, false);
    setTimeout(function () { wasm_exports.key_up(code, 0); }, KEY_PULSE_MS);
  }

  /**
   * Remap joystick x-deflection to a walk speed factor.
   *
   * Dead zone  : |nx| < JOY_DEAD_ZONE     → 0 (no movement)
   * Active zone: JOY_DEAD_ZONE ≤ |nx| ≤ 1 → JOY_MIN_WALK … 1.0
   *
   * The minimum non-zero output (JOY_MIN_WALK) ensures the ball visibly moves
   * even at the lowest joystick deflection so the control feels responsive.
   */
  function analogFromNx(nx) {
    var abs = Math.abs(nx);
    if (abs < JOY_DEAD_ZONE) return 0.0;
    var sign = nx > 0 ? 1 : -1;
    var t = (abs - JOY_DEAD_ZONE) / (1.0 - JOY_DEAD_ZONE); /* 0…1 past the dead zone */
    return sign * (JOY_MIN_WALK + t * JOY_WALK_RANGE);
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

    /* ── JUMP button (to the right of the joystick) ── */
    var jumpBtn = mkBtn("⬆\nJUMP", {
      bottom: "80px", left: "155px", w: "75px", h: "75px",
      bg: "rgba(15,65,25,0.90)", border: "rgba(55,190,80,0.95)",
      fontSize: "14px",
    });
    ov.appendChild(jumpBtn);
    jumpBtn.addEventListener("touchstart", function (e) {
      e.preventDefault(); e.stopPropagation();
      pulseKey(KEY_SPACE);
    }, false);

    /* ── Weapon menu button (bottom-right) ── */
    var weaponBtn = mkBtn("🔫\nWEAPON", {
      bottom: "150px", right: "100px", w: "80px", h: "60px",
    });
    ov.appendChild(weaponBtn);
    tapKey(weaponBtn, KEY_TAB);
    /* Also keep our JS-side menuOpen flag in sync */
    weaponBtn.addEventListener("touchstart", function (e) {
      e.stopPropagation();
      menuOpen = !menuOpen;
    }, false);

    /* ── FIRE button (big, bottom-right) ── */
    var fireBtn = mkBtn("🔥\nFIRE", {
      bottom: "80px", right: "10px", w: "80px", h: "130px",
      bg: "rgba(120,25,15,0.90)", border: "rgba(230,80,60,0.95)",
      fontSize: "16px",
    });
    ov.appendChild(fireBtn);
    setupFireButton(fireBtn);

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
    /* held.up tracks whether the up-jump gesture has already fired this stroke */
    var held = { up: false };

    /* Stop analog walk and release any held digital fallback keys */
    function stopWalk() {
      if (typeof wasm_exports.set_analog_walk === "function") {
        wasm_exports.set_analog_walk(0.0);
      } else {
        wasm_exports.key_up(KEY_LEFT, 0);
        wasm_exports.key_up(KEY_RIGHT, 0);
      }
    }

    function reset() {
      active = false;
      touchId = null;
      js.thumb.style.transform = "translate(-50%,-50%)";
      stopWalk();
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

        /* ── Horizontal movement: analog proportional speed ── */
        if (typeof wasm_exports.set_analog_walk === "function") {
          wasm_exports.set_analog_walk(analogFromNx(nx));
        } else {
          /* Fallback: binary keys for older WASM builds */
          if (nx < -0.25) {
            wasm_exports.key_down(KEY_LEFT, 0, false);
            wasm_exports.key_up(KEY_RIGHT, 0);
          } else if (nx > 0.25) {
            wasm_exports.key_down(KEY_RIGHT, 0, false);
            wasm_exports.key_up(KEY_LEFT, 0);
          } else {
            wasm_exports.key_up(KEY_LEFT, 0);
            wasm_exports.key_up(KEY_RIGHT, 0);
          }
        }

        /* ── Up gesture → jump (one-shot per stroke) ── */
        if (ny < -0.5 && !held.up) {
          held.up = true;
          pulseKey(KEY_SPACE);
        }
        /* Reset so next upward push can jump again */
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

  /* ── FIRE button ───────────────────────────────────────────────────────────
   *
   * hold = charge (mouse_down held), release = fire (mouse_up).
   *
   * While the button is held, `fireButtonDown = true` which suppresses canvas
   * mouse_move events.  This keeps mouse_position() stable in Rust and prevents
   * the left_drag_panning guard from cancelling the charge mid-flight.
   */
  function setupFireButton(btn) {
    btn.addEventListener("touchstart", function (e) {
      e.preventDefault(); e.stopPropagation();
      fireButtonDown = true;
      wasm_exports.mouse_down(lastAimX, lastAimY, 0);
    }, false);

    function fireRelease(e) {
      e.preventDefault(); e.stopPropagation();
      fireButtonDown = false;
      wasm_exports.mouse_up(lastAimX, lastAimY, 0);
    }
    btn.addEventListener("touchend",    fireRelease, false);
    btn.addEventListener("touchcancel", fireRelease, false);
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
   *   • Menu CLOSED  → mouse_move (aim) ONLY — NEVER mouse_down/up
   *                    (mouse_move is suppressed while fireButtonDown is true
   *                     to prevent left_drag_panning from cancelling an active charge)
   *   • Menu OPEN    → drag scrolls the list via mouse_wheel
   *                    tap (< TAP_MOVE_THRESHOLD px movement) sends mouse_down+up
   *                    to select a weapon or close the menu
   *                    (suppressed if fireButtonDown to prevent accidental double-fire)
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
        /* Only send mouse_move when FIRE is not held — prevents mouse_position()
         * from drifting away from the charge origin (would trigger left_drag_panning). */
        if (!fireButtonDown) {
          wasm_exports.mouse_move(p.x, p.y);
        }
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

          if (gestureMode === 'aim') {
            /* Always update lastAimX/Y so the FIRE button fires at the latest aim position. */
            lastAimX = p.x; lastAimY = p.y;
            /* Only forward mouse_move to WASM when FIRE is not held.
             * Suppressing during charge prevents mouse_position() from drifting,
             * which would otherwise trigger left_drag_panning and cancel the charge. */
            if (!fireButtonDown) {
              wasm_exports.mouse_move(p.x, p.y);
            }
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
        /* Weapon selection tap: send a single left-click at the tap position.
         * Guarded by !fireButtonDown to prevent accidental clicks while charging. */
        if (menuOpen && !isDragging && !fireButtonDown) {
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
  }

  /* ── Real two-zone single-finger handler ─────────────────────────────────────
   * A pre-capture listener fires before setupCanvasTouches and decides the mode.
   * If the touch starts far from the current aim point it starts a right-button pan;
   * otherwise it falls through to the aim handler above.
   */
  function setupCanvasTwoZone(canvas) {
    /* Aim position as of the START of each gesture (not updated during gesture) */
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


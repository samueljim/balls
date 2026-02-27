/**
 * Miniquad plugin: WebSocket bridge for game-core WASM.
 * Reads gameId, playerId from URL; playerOrder from sessionStorage.
 * Registers send_ws_message(ptr, len) for Rust; on WS message calls on_ws_message(ptr, len).
 * Call on_game_init(ptr, len) with init JSON after connect (or when first state arrives).
 */
(function () {
  "use strict";

  var ws = null;
  var gameId = "";
  var playerId = "";
  var playerOrder = [];
  var lobbyRngSeed = null; // Seed from lobby (set at game start by host)

  function getApiBase() {
    if (typeof window !== "undefined" && window.__WORMS_WS_BASE) return window.__WORMS_WS_BASE;
    return "https://api.worms.bne.sh";
  }

  function getHttpBase() {
    // Convert ws:// to http:// and wss:// to https:// for fetch requests
    var base = getApiBase();
    return base.replace(/^ws:/, "http:").replace(/^wss:/, "https:");
  }

  function getWsUrl(path) {
    return getApiBase().replace(/^http/, "ws") + path;
  }

  function parsePageContext() {
    if (typeof window === "undefined") return;
    var path = window.location.pathname || "";
    var match = path.match(/\/game\/([^/]+)/);
    gameId = match ? match[1] : "";
    var params = new URLSearchParams(window.location.search || "");
    playerId = params.get("playerId") || "";
    try {
      var key = "worms:" + gameId;
      var stored = window.sessionStorage && sessionStorage.getItem(key);
      if (stored) {
        var parsed = JSON.parse(stored);
        if (Array.isArray(parsed)) {
          playerOrder = parsed;
        } else if (parsed && Array.isArray(parsed.playerOrder)) {
          playerOrder = parsed.playerOrder;
          if (typeof parsed.rngSeed === "number") {
            lobbyRngSeed = parsed.rngSeed;
          }
        }
      }
    } catch (e) {
      playerOrder = [];
    }
  }

  function UTF8ToString(heap, ptr, len) {
    var out = "";
    for (var i = 0; i < len; i++) {
      var c = heap[ptr + i];
      if (!c) break;
      out += String.fromCharCode(c);
    }
    return decodeURIComponent(escape(out));
  }

  function stringToUTF8(str, heap, ptr, maxLen) {
    var s = unescape(encodeURIComponent(str));
    var len = Math.min(s.length, maxLen);
    for (var i = 0; i < len; i++) heap[ptr + i] = s.charCodeAt(i);
    return len;
  }

  function register_plugin(importObject) {
    parsePageContext();

    importObject.env.js_send_ws = function (ptr, len) {
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        console.warn("[ws_plugin] Cannot send - WebSocket not open");
        return;
      }
      if (typeof wasm_memory === "undefined") return;
      try {
        var heap = new Uint8Array(wasm_memory.buffer, ptr, len);
        var str = UTF8ToString(heap, 0, len);
        console.log("[ws_plugin] Sending:", str.substring(0, 100)); // Log first 100 chars
        ws.send(str);
      } catch (e) {
        console.warn("[ws_plugin] send failed", e);
      }
    };
  }

  function on_init() {
    if (!gameId || !playerOrder.length) return;
    console.log("[ws_plugin] on_init called: gameId=" + gameId + ", playerOrder.length=" + playerOrder.length);
    ws = new WebSocket(getWsUrl("/game/" + gameId + "?playerId=" + encodeURIComponent(playerId)));
    console.log("[ws_plugin] WebSocket created: " + getWsUrl("/game/" + gameId + "?playerId=" + encodeURIComponent(playerId)));

    var names = [];
    var bots = [];
    var serverMyPlayerIndex = null;
    var serverRngSeed = null;
    // Calculate fallback seed from gameId (used only if server doesn't provide one)
    var fallbackSeed = 0;
    for (var i = 0; i < gameId.length; i++) fallbackSeed = ((fallbackSeed << 5) - fallbackSeed + gameId.charCodeAt(i)) | 0;
    fallbackSeed = Math.abs(fallbackSeed >>> 0);

    // Prepare player names and bot flags
    if (Array.isArray(playerOrder)) {
      for (var i = 0; i < playerOrder.length; i++) {
        var p = playerOrder[i];
        var name = (p && p.name) || ("Player " + (i + 1));
        var isBot = (p && p.isBot) ? true : false;
        names.push(name);
        bots.push(isBot ? "1" : "0");
      }
    }

    function sendGameInit() {
      if (serverMyPlayerIndex === null || serverRngSeed === null) return; // Wait for server identity and seed
      console.log("[ws_plugin] sendGameInit: myPlayerIndex=" + serverMyPlayerIndex + ", seed=" + serverRngSeed);
      var initData = JSON.stringify({
        gameId: gameId,
        playerId: playerId,
        playerOrder: playerOrder,
        rngSeed: serverRngSeed,
        myPlayerIndex: serverMyPlayerIndex,
        playerNames: names.join(","),
        playerBots: bots.join(","),
      });
      if (typeof wasm_exports !== "undefined" && wasm_exports.on_game_init) {
        var buf = new TextEncoder().encode(initData);
        var ptr = wasm_exports.alloc_buffer(buf.length);
        if (ptr) {
          new Uint8Array(wasm_memory.buffer, ptr, buf.length).set(buf);
          wasm_exports.on_game_init(ptr, buf.length);
          console.log("[ws_plugin] Called wasm on_game_init");
        }
      } else {
        console.warn("[ws_plugin] WASM not ready! wasm_exports=" + typeof wasm_exports);
      }
    }

    ws.onopen = function () {
      console.log("[ws_plugin] WebSocket OPENED");
      // Use seed from lobby (via sessionStorage from game_started), then server identity, then fallback
      var seedToSend = lobbyRngSeed !== null ? lobbyRngSeed : (serverRngSeed !== null ? serverRngSeed : fallbackSeed);
      console.log("[ws_plugin] POST /init with rngSeed=" + seedToSend);
      fetch(getHttpBase() + "/game/" + gameId + "/init", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ playerOrder: playerOrder, rngSeed: seedToSend, terrainId: 0 }),
      }).then(function(r) { 
        console.log("[ws_plugin] /init response:", r.status);
      }).catch(function (e) { 
        console.warn("[ws_plugin] init POST failed", e); 
      });
    };

    ws.onmessage = function (event) {
      var data = typeof event.data === "string" ? event.data : new TextDecoder().decode(event.data);
      
      // Handle identity message from server
      try {
        var parsed = JSON.parse(data);
        if (parsed.type === "identity" && typeof parsed.myPlayerIndex === "number") {
          console.log("[ws_plugin] Received identity: myPlayerIndex=" + parsed.myPlayerIndex + ", rngSeed=" + parsed.rngSeed);
          serverMyPlayerIndex = parsed.myPlayerIndex;
          // Extract authoritative seed from server
          if (typeof parsed.rngSeed === "number") {
            serverRngSeed = parsed.rngSeed;
          }
          sendGameInit();
          return;
        }
      } catch (e) {}
      
      // Forward all messages to WASM
      if (typeof wasm_exports === "undefined" || !wasm_exports.on_ws_message) return;
      var buf = new TextEncoder().encode(data);
      var ptr = wasm_exports.alloc_buffer(buf.length);
      if (!ptr) return;
      new Uint8Array(wasm_memory.buffer, ptr, buf.length).set(buf);
      wasm_exports.on_ws_message(ptr, buf.length);
    };

    ws.onclose = function (event) {
      console.warn("[ws_plugin] WebSocket CLOSED: code=" + event.code + ", reason=" + event.reason);
    };
    ws.onerror = function (err) {
      console.error("[ws_plugin] WebSocket ERROR:", err);
    };
  }

  if (typeof miniquad_add_plugin !== "undefined") {
    miniquad_add_plugin({
      register_plugin: register_plugin,
      on_init: on_init,
    });
  }
})();

import assert from "node:assert/strict";
import { test } from "node:test";
import { queryArguments, startup_arguments } from "./options.js";

test("empty queries and unrelated parameters preserve defaults", () => {
  assert.deepEqual(queryArguments(""), []);
  assert.deepEqual(queryArguments("?utm_source=link&constructor=text&toString=text"), []);
});

test("combined options become CLI arguments in URL order", () => {
  assert.deepEqual(
    queryArguments("?renderer=3d-walls&motion=ease%2Din%2Dout&map=portals&reveal&portal-view=north"),
    ["--renderer", "3d-walls", "--motion", "ease-in-out", "--map", "assets/maps/portals.txt", "--reveal", "--portal-view", "north"],
  );
});

test("all renderer, motion and portal presets are accepted", () => {
  for (const [name, values] of Object.entries({
    renderer: ["text", "3d-walls"],
    motion: ["locomotion", "ease-out", "linear", "ease-in-out", "overshoot", "snap"],
    "portal-view": ["turn", "north"],
  })) {
    for (const value of values) {
      assert.deepEqual(queryArguments(`?${name}=${value}`), [`--${name}`, value]);
    }
  }
});

test("bundled maps accept short names, filenames and encoded CLI paths", () => {
  for (const map of ["map1", "map1_test", "map2", "map3", "lightTest", "portals", "hunter_keys", "chasers"]) {
    for (const value of [map, `${map}.txt`, `assets/maps/${map}.txt`]) {
      assert.deepEqual(queryArguments(`?map=${encodeURIComponent(value)}`), ["--map", `assets/maps/${map}.txt`]);
    }
  }
});

test("reveal supports bare, true/false and 1/0 values", () => {
  for (const value of ["", "true", "1"]) assert.deepEqual(queryArguments(`?reveal=${value}`), ["--reveal"]);
  for (const value of ["false", "0"]) assert.deepEqual(queryArguments(`?reveal=${value}`), []);
});

test("invalid, empty and duplicate supported options fail clearly", () => {
  for (const query of ["renderer=bad", "motion=bad", "portal-view=bad", "map=bad", "reveal=bad", "renderer=", "motion=", "portal-view=", "map=", "map=../map1", "map=https://example.com/map1.txt"]) {
    assert.throws(() => queryArguments(`?${query}`), /must be/);
  }
  for (const name of ["renderer", "motion", "portal-view", "map", "reveal"]) {
    const value = { renderer: "text", motion: "snap", "portal-view": "turn", map: "map1", reveal: "false" }[name];
    assert.throws(() => queryArguments(`?${name}=${value}&${name}=${value}`), /Duplicate URL option/);
  }
});

test("native-only options are rejected even with empty values", () => {
  for (const name of ["screenshot", "record", "walk", "remote", "remote-port"]) {
    assert.throws(() => queryArguments(`?${name}`), /only available in the native version/);
  }
});

test("the Wasm bridge reads the browser location and returns a JSON array", () => {
  globalThis.window = { location: { search: "?motion=snap&reveal=1" } };
  try {
    assert.deepEqual(JSON.parse(startup_arguments()), ["--motion", "snap", "--reveal"]);
  } finally {
    delete globalThis.window;
  }
});

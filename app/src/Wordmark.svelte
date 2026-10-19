<script>
  // romforge's name as its logo: original pixel lettering (7×7 cells per
  // letter, chunky strokes, a highlight in each letter's top-left corner),
  // each letter in a UK SNES button colour in the buttons' order (red,
  // yellow, green, blue), darker at its foot, with a console-grey drop
  // shadow. Drawn as SVG cells, so it stays sharp at any size.
  let { height = "1.6rem" } = $props();

  const GLYPHS = {
    R: ["XXXXXX.", "XX...XX", "XX...XX", "XXXXXX.", "XX.XX..", "XX..XX.", "XX...XX"],
    O: [".XXXXX.", "XX...XX", "XX...XX", "XX...XX", "XX...XX", "XX...XX", ".XXXXX."],
    M: ["XX...XX", "XXX.XXX", "XXXXXXX", "XX.X.XX", "XX...XX", "XX...XX", "XX...XX"],
    F: ["XXXXXXX", "XX.....", "XX.....", "XXXXX..", "XX.....", "XX.....", "XX....."],
    G: [".XXXXXX", "XX.....", "XX.....", "XX..XXX", "XX...XX", "XX...XX", ".XXXXX."],
    E: ["XXXXXXX", "XX.....", "XX.....", "XXXXX..", "XX.....", "XX.....", "XXXXXXX"],
  };
  // [colour, its darker shade] per button.
  const BUTTONS = [
    ["#d8312f", "#a8221f"],
    ["#f0b81d", "#c48f0f"],
    ["#1f9b4f", "#157338"],
    ["#3b5ab0", "#2a428a"],
  ];
  const SHADOW = "#3a3a44";
  const WORD = "ROMFORGE";

  // Cells of every letter, a letter 7 wide with one cell between.
  const cells = [];
  const marks = [];
  let x = 0;
  [...WORD].forEach((ch, i) => {
    const g = GLYPHS[ch];
    const [light, dark] = BUTTONS[i % BUTTONS.length];
    g.forEach((row, r) => [...row].forEach((v, c) => v === "X" && cells.push([x + c, r, r >= 5 ? dark : light])));
    marks.push([x + g[0].indexOf("X"), 0]);
    x += g[0].length + 1;
  });
  const width = x - 1 + 0.5;
</script>

<svg class="wordmark" style:height viewBox="0 0 {width} 7.5" role="img" aria-label="romforge" shape-rendering="crispEdges">
  {#each cells as [cx, cy]}<rect x={cx + 0.5} y={cy + 0.5} width="1" height="1" fill={SHADOW} />{/each}
  {#each cells as [cx, cy, fill]}<rect x={cx} y={cy} width="1.02" height="1.02" {fill} />{/each}
  {#each marks as [mx, my]}<path d="M{mx + 0.35} {my + 1.3}V{my + 0.35}H{mx + 1.3}" fill="none" stroke="#fff8e0" stroke-width="0.3" />{/each}
</svg>

<style>
  .wordmark { display: block; width: auto; }
</style>

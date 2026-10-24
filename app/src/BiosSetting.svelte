<script>
  // Settings, Rename & Cleanup tab: whether BIOS files imported into
  // RomForge's Bios folder stay where they were too.
  let { draft = $bindable() } = $props();
</script>

<section class="setting">
  <span class="flabel">BIOS files</span>
  <label class="toggle">
    <input type="checkbox" bind:checked={draft.bios_leave_in_place} />
    Leave BIOS files where they are
  </label>
  <p class="hint">
    {draft.bios_leave_in_place
      ? "BIOS files you import (dropped, or found on this computer) are copied into RomForge's Bios folder; the originals stay where they are."
      : "BIOS files you import are copied into RomForge's Bios folder, then each original goes to the trash once its copy is checked (Undo on the BIOS page puts them back)."}
  </p>
</section>

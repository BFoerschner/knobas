<!--
  The controls specimen (#2280): one example of every control and arrangement
  the design system's Controls section names, drawn with the real stylesheet
  and no style of its own. Nothing here does anything: it is the picture a UI
  ticket is held to. A control added to the section is added here in the same
  PR (`TfControls.test.svelte.ts` reads the section's table).
-->
<script lang="ts">
  let layout = $state<"board" | "list">("board");
  let mine = $state(true);
  let failing = $state(false);
</script>

<div class="tf-full">
  <header class="tf-fhead">
    <div>
      <h1>Controls</h1>
      <div class="tf-sub">The canonical header: primary, the other actions, Open in, then Sync now last.</div>
    </div>
    <div class="tf-acts">
      <button type="button" class="tf-btn tf-primary">New pull request</button>
      <button type="button" class="tf-btn">Edit</button>
      <button type="button" class="tf-btn tf-danger">Delete</button>
      <button type="button" class="tf-btn tf-ghost">Open in Gitea</button>
      <button type="button" class="tf-btn">Sync now</button>
    </div>
  </header>

  <div class="tf-wtabs" role="tablist" aria-label="Tab strip">
    <a href="#/controls" role="tab" aria-selected="true">Overview</a>
    <a href="#/controls" role="tab" aria-selected="false">Code</a>
    <a href="#/controls" role="tab" aria-selected="false">Conversation</a>
    <div class="tf-trail">
      <button type="button" class="tf-btn tf-sm">Connections <kbd>L</kbd></button>
      <button type="button" class="tf-btn tf-sm">Refresh</button>
    </div>
  </div>

  <div class="tf-fbar">
    <label class="tf-fsearch">
      <input type="text" placeholder="Filter" aria-label="Filter" />
    </label>
    <label class="tf-fsel">
      Branch
      <select aria-label="Branch">
        <option>main</option>
        <option>release/2.4</option>
      </select>
    </label>
    <div class="tf-seg" role="group" aria-label="Layout">
      <button type="button" aria-pressed={layout === "board"} onclick={() => (layout = "board")}>Board</button>
      <button type="button" aria-pressed={layout === "list"} onclick={() => (layout = "list")}>List</button>
    </div>
    <div class="tf-fchips" role="group" aria-label="Show only">
      <button type="button" aria-pressed={mine} onclick={() => (mine = !mine)}>Mine</button>
      <button type="button" aria-pressed={failing} onclick={() => (failing = !failing)}>Failing</button>
    </div>
    <div class="tf-trail">
      <button type="button" class="tf-btn tf-sm">Save view</button>
    </div>
  </div>

  <div class="tf-wbody">
    <section aria-labelledby="tf-ctl-section">
      <div class="tf-ext-h">
        <h2 id="tf-ctl-section">A section heading with a small action</h2>
        <button type="button" class="tf-btn tf-sm">Check for updates</button>
      </div>
      <div class="tf-ext-row">
        <span class="tf-stg-sn">
          <b>A row</b>
          <span class="tf-dim">Small actions and an icon-only button, square at the row's height.</span>
        </span>
        <div class="tf-trail">
          <button type="button" class="tf-btn tf-sm">Open</button>
          <button type="button" class="tf-btn tf-sm tf-danger">Delete</button>
          <button type="button" class="tf-iconbtn tf-sm" aria-label="Toggle colour theme">
            <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
              <path d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1zm0 12.5v-11a5.5 5.5 0 0 1 0 11z" />
            </svg>
          </button>
        </div>
      </div>
    </section>

    <section class="tf-dialog" aria-labelledby="tf-ctl-dialog">
      <header><h2 id="tf-ctl-dialog">A dialog footer</h2></header>
      <div class="tf-body"><p>The extra action on the far left, then a ghost Cancel, then the primary.</p></div>
      <footer>
        <button type="button" class="tf-btn tf-ghost tf-danger tf-first">Delete</button>
        <button type="button" class="tf-btn tf-ghost">Cancel</button>
        <button type="button" class="tf-btn tf-primary">Save</button>
      </footer>
    </section>

    <section class="tf-dialog" aria-labelledby="tf-ctl-danger">
      <header><h2 id="tf-ctl-danger">The danger pair</h2></header>
      <div class="tf-body">
        <p>Red text starts a destructive action; filled red confirms it.</p>
        <div><button type="button" class="tf-btn tf-danger">Delete note</button></div>
      </div>
      <footer>
        <button type="button" class="tf-btn tf-ghost">Cancel</button>
        <button type="button" class="tf-btn tf-primary tf-danger">Delete</button>
      </footer>
    </section>

    <section aria-labelledby="tf-ctl-decide">
      <h2 id="tf-ctl-decide" class="tf-pane-h">The one decision a screen exists for</h2>
      <div class="tf-decide">
        <button type="button" class="tf-btn tf-lg tf-yes">Link them <kbd>Y</kbd></button>
        <button type="button" class="tf-btn tf-lg tf-no">Not related <kbd>N</kbd></button>
        <button type="button" class="tf-btn tf-lg tf-ghost">Decide later <kbd>S</kbd></button>
      </div>
    </section>
  </div>
</div>

<script lang="ts">
  // Vista Approvals del vault. La misma lista se puede abrir como pestaña (que se arrastra
  // a otra ventana o al lado de una terminal) o directo en una ventana propia.
  import { AppWindow, PanelTop } from "@lucide/svelte";
  import ApprovalList from "../components/ApprovalList.svelte";
  import { openApprovals } from "../lib/state.svelte";
  import { approvalsWindow } from "../lib/windows";
</script>

<div class="page">
  <div class="page-head">
    <h1>Approvals</h1>
    <span class="sub">Commands from agents on hosts in Approval mode wait here.</span>
    <span class="grow"></span>
    <button class="btn" title="Open as a tab next to your terminals. Drag it onto a terminal to see both at once." onclick={openApprovals}>
      <PanelTop size={14} /> Open as tab
    </button>
    <button class="btn" title="Open in a window of its own" onclick={approvalsWindow}>
      <AppWindow size={14} /> Open in new window
    </button>
  </div>

  <ApprovalList />
</div>

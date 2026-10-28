<script lang="ts" module>
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Clock from "@lucide/svelte/icons/clock";
  import Download from "@lucide/svelte/icons/download";
  import Folder from "@lucide/svelte/icons/folder";
  import Globe from "@lucide/svelte/icons/globe";
  import HardDriveDownload from "@lucide/svelte/icons/hard-drive-download";
  import House from "@lucide/svelte/icons/house";
  import Image from "@lucide/svelte/icons/image";
  import Info from "@lucide/svelte/icons/info";
  import Languages from "@lucide/svelte/icons/languages";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import Minimize2 from "@lucide/svelte/icons/minimize-2";
  import Minus from "@lucide/svelte/icons/minus";
  import Palette from "@lucide/svelte/icons/palette";
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";
  import Plus from "@lucide/svelte/icons/plus";
  import RectangleHorizontal from "@lucide/svelte/icons/rectangle-horizontal";
  import RectangleVertical from "@lucide/svelte/icons/rectangle-vertical";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Search from "@lucide/svelte/icons/search";
  import Settings from "@lucide/svelte/icons/settings";
  import Square from "@lucide/svelte/icons/square";
  import Store from "@lucide/svelte/icons/store";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Wrench from "@lucide/svelte/icons/wrench";
  import X from "@lucide/svelte/icons/x";

  const icons = {
    "home": House,
    "info": Info,
    "library": LibraryBig,
    "landscape": RectangleHorizontal,
    "portrait": RectangleVertical,
    "store": Store,
    "downloads": HardDriveDownload,
    "settings": Settings,
    "folder": Folder,
    "image": Image,
    "wrench": Wrench,
    "warning": TriangleAlert,
    "danger": CircleAlert,
    "success": CircleCheck,
    "play": Play,
    "pause": Pause,
    "download": Download,
    "stop": Square,
    "clock": Clock,
    "close": X,
    "previous": ChevronLeft,
    "next": ChevronRight,
    "reload": RotateCw,
    "search": Search,
    "chevron-down": ChevronDown,
    "plus": Plus,
    "check": Check,
    "palette": Palette,
    "globe": Globe,
    "translate": Languages,
    "back": ArrowLeft,
    "minimize": Minus,
    "maximize": Maximize2,
    "restore": Minimize2,
  };

  export type IconName = keyof typeof icons;

  // Media controls read better as solid shapes at small sizes.
  const filled: ReadonlySet<IconName> = new Set(["play", "pause", "stop"]);
</script>

<script lang="ts">
  let { name, size = "h-5 w-5" }: { name: IconName; size?: string } = $props();

  const Component = $derived(icons[name]);
</script>

<Component class="{size} {filled.has(name) ? 'fill-current' : ''}" aria-hidden="true" />

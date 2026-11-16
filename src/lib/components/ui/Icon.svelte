<script lang="ts" module>
  import AppWindow from "@lucide/svelte/icons/app-window";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Bell from "@lucide/svelte/icons/bell";
  import Calendar from "@lucide/svelte/icons/calendar";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleArrowUp from "@lucide/svelte/icons/circle-arrow-up";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Clock from "@lucide/svelte/icons/clock";
  import Copy from "@lucide/svelte/icons/copy";
  import Download from "@lucide/svelte/icons/download";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import FileCode from "@lucide/svelte/icons/file-code";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Funnel from "@lucide/svelte/icons/funnel";
  import Gamepad2 from "@lucide/svelte/icons/gamepad-2";
  import Gauge from "@lucide/svelte/icons/gauge";
  import Globe from "@lucide/svelte/icons/globe";
  import HardDriveDownload from "@lucide/svelte/icons/hard-drive-download";
  import Heart from "@lucide/svelte/icons/heart";
  import House from "@lucide/svelte/icons/house";
  import Image from "@lucide/svelte/icons/image";
  import Info from "@lucide/svelte/icons/info";
  import Languages from "@lucide/svelte/icons/languages";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import List from "@lucide/svelte/icons/list";
  import ArrowDownWideNarrow from "@lucide/svelte/icons/arrow-down-wide-narrow";
  import ArrowUpNarrowWide from "@lucide/svelte/icons/arrow-up-narrow-wide";
  import FolderHeart from "@lucide/svelte/icons/folder-heart";
  import Keyboard from "@lucide/svelte/icons/keyboard";
  import Link from "@lucide/svelte/icons/link";
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import Minimize2 from "@lucide/svelte/icons/minimize-2";
  import Minus from "@lucide/svelte/icons/minus";
  import Package from "@lucide/svelte/icons/package";
  import Palette from "@lucide/svelte/icons/palette";
  import PanelsTopLeft from "@lucide/svelte/icons/panels-top-left";
  import Pause from "@lucide/svelte/icons/pause";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Play from "@lucide/svelte/icons/play";
  import Plus from "@lucide/svelte/icons/plus";
  import Power from "@lucide/svelte/icons/power";
  import Puzzle from "@lucide/svelte/icons/puzzle";
  import RectangleHorizontal from "@lucide/svelte/icons/rectangle-horizontal";
  import RectangleVertical from "@lucide/svelte/icons/rectangle-vertical";
  import Rocket from "@lucide/svelte/icons/rocket";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import Search from "@lucide/svelte/icons/search";
  import Settings from "@lucide/svelte/icons/settings";
  import Share2 from "@lucide/svelte/icons/share-2";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Square from "@lucide/svelte/icons/square";
  import Star from "@lucide/svelte/icons/star";
  import Store from "@lucide/svelte/icons/store";
  import Terminal from "@lucide/svelte/icons/terminal";
  import ThumbsUp from "@lucide/svelte/icons/thumbs-up";
  import Timer from "@lucide/svelte/icons/timer";
  import Trash from "@lucide/svelte/icons/trash";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Upload from "@lucide/svelte/icons/upload";
  import Volume2 from "@lucide/svelte/icons/volume-2";
  import VolumeX from "@lucide/svelte/icons/volume-x";
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
    "calendar": Calendar,
    "check": Check,
    "palette": Palette,
    "globe": Globe,
    "translate": Languages,
    "back": ArrowLeft,
    "minimize": Minus,
    "maximize": Maximize2,
    "restore": Minimize2,
    "mods": Puzzle,
    "package": Package,
    "trash": Trash,
    "upload": Upload,
    "share": Share2,
    "file-code": FileCode,
    "folder-open": FolderOpen,
    "pencil": Pencil,
    "copy": Copy,
    "external-link": ExternalLink,
    "thumbs-up": ThumbsUp,
    "funnel": Funnel,
    "heart": Heart,
    "volume": Volume2,
    "muted": VolumeX,
    "sliders": SlidersHorizontal,
    "integrity": ShieldCheck,
    "bell": Bell,
    "timer": Timer,
    "speed": Gauge,
    "gamepad": Gamepad2,
    "power": Power,
    "keyboard": Keyboard,
    "link": Link,
    "hidden": EyeOff,
    "logs": ScrollText,
    "terminal": Terminal,
    "launch": Rocket,
    "panels": PanelsTopLeft,
    "window": AppWindow,
    "update": CircleArrowUp,
    "star": Star,
    "collection": FolderHeart,
    "grid": LayoutGrid,
    "list": List,
    "sort-ascending": ArrowUpNarrowWide,
    "sort-descending": ArrowDownWideNarrow,
  };

  export type IconName = keyof typeof icons;

  // Media controls read better as solid shapes at small sizes.
  const filled: ReadonlySet<IconName> = new Set(["play", "pause", "stop"]);

  export type IconSize = "sm" | "md" | "lg" | "xl";

  // 14, 16 and 20 px follow the control sizes; 24 px is for empty states and hero actions.
  const sizes: Record<IconSize, string> = { sm: "size-3.5", md: "size-4", lg: "size-5", xl: "size-6" };
</script>

<script lang="ts">
  let { name, size = "md", class: className = "" }: { name: IconName; size?: IconSize; class?: string } = $props();

  const Component = $derived(icons[name]);
</script>

<Component class="{sizes[size]} {filled.has(name) ? 'fill-current' : ''} {className}" aria-hidden="true" />

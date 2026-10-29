import {
  createIcons,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronsRight,
  CirclePlus,
  Crop,
  FileImage,
  FileText,
  Highlighter,
  Image,
  Info,
  LayoutGrid,
  Menu,
  Minus,
  PanelLeft,
  PenLine,
  Pencil,
  PenTool,
  RectangleEllipsis,
  RotateCcwSquare,
  Search,
  Shapes,
  Share,
  Signature,
  SlidersHorizontal,
  Square,
  SquareDashed,
  StickyNote,
  TextCursor,
  TextCursorInput,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide";

/** Remplace les <i data-lucide="…"> par les icônes Lucide (trait fin de 1,5 px, comme la charte). */
export function drawIcons() {
  createIcons({
    icons: {
      ChevronDown, ChevronLeft, ChevronRight, ChevronsRight, CirclePlus, Crop, FileImage, FileText, Highlighter, Image, Info, LayoutGrid, Menu, Minus, PanelLeft, PenLine,
      Pencil, PenTool, RectangleEllipsis, RotateCcwSquare, Search, Shapes, Share, Signature, SlidersHorizontal, Square,
      SquareDashed, StickyNote, TextCursor, TextCursorInput, X, ZoomIn, ZoomOut,
    },
    attrs: { "stroke-width": 1.5 },
  });
}

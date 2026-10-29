// Lien avec le cœur Rust (src-tauri). Tout le travail lourd se fait là-bas, dans un fil à part.
import { invoke } from "@tauri-apps/api/core";

export type Kind = "pdf" | "jpeg" | "png" | "heic" | "webp" | "gif";

export interface PageSize {
  width: number;
  height: number;
}

export interface DocInfo {
  id: number;
  name: string;
  path: string;
  kind: Kind;
  pages: PageSize[];
  /** Photo retouchée en mémoire (recadrée, couleurs…). */
  edited: boolean;
}

/** Retouche d'une photo : réglages de -1 à 1, recadrage en pixels de la photo redressée. */
export interface Retouch {
  crop?: { x: number; y: number; w: number; h: number };
  brightness?: number;
  contrast?: number;
  saturation?: number;
  flip_h?: boolean;
  flip_v?: boolean;
  /** Nouvelle taille en pixels (« Ajuster la taille »). */
  resize?: [number, number];
}

export interface NormRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface SearchHit {
  doc: number;
  page: number;
  rects: NormRect[];
  context: string;
}

export interface OpenResult {
  docs: DocInfo[];
  errors: { name: string; message: string }[];
}

export const inTauri = "__TAURI_INTERNALS__" in window;

export const openPaths = (paths: string[]) => invoke<OpenResult>("open_paths", { paths });
export const closeDocs = (ids: number[]) => invoke<void>("close_docs", { ids });
export const search = (query: string) => invoke<SearchHit[]>("search", { query });
export interface PageRef {
  doc: number;
  page: number;
  rotation: number;
  annotations?: unknown[];
}

export interface PageText {
  chars: string[];
  rects: NormRect[];
}

export const pageText = (doc: number, page: number) => invoke<PageText>("page_text", { doc, page });

/** Ce que montre l'inspecteur : le fichier, et les informations d'un PDF. */
export interface DocDetails {
  file_size: number;
  file_created: number | null;
  file_modified: number | null;
  version: string | null;
  title: string | null;
  author: string | null;
  subject: string | null;
  keywords: string | null;
  creator: string | null;
  producer: string | null;
  created: string | null;
  modified: string | null;
}

export const docDetails = (doc: number) => invoke<DocDetails>("doc_details", { doc });

/** Photo annotée : l'image finale part telle quelle (PNG), sans passer par du JSON. */
export const savePicturePixels = (doc: number, png: Uint8Array, dest: string) =>
  invoke<void>("save_picture_pixels", png, {
    headers: { "x-doc": String(doc), "x-dest": encodeURIComponent(dest) },
  });

export interface FormField {
  id: string;
  name: string;
  kind: "text" | "checkbox" | "radio" | "choice";
  page: number;
  rect: { x: number; y: number; w: number; h: number };
  value: string;
  on: string;
  options: string[];
  multiline: boolean;
  comb: boolean;
  max_len: number | null;
  read_only: boolean;
  font_size: number;
  courier: boolean;
  align: number;
  color: string;
  units_per_point: number;
}

export type FieldFill = { id: string } & (
  | { kind: "text"; text: string; lines: { text: string; width: number; chars: number[] }[]; size: number }
  | { kind: "check"; on: boolean }
  | { kind: "radio"; value: string | null }
);

export interface DocFill {
  doc: number;
  fields: FieldFill[];
}

/** Photo d'une signature → image transparente (data URL). */
export const signatureFromPhoto = (path: string) => invoke<string>("signature_from_photo", { path });
export type ReduceLevel = "moyenne" | "forte";
/** Enregistre un PDF allégé ; renvoie [taille avant, taille après] en octets. */
export const savePdfReduced = (pages: PageRef[], dest: string, level: ReduceLevel, fills: DocFill[] = []) =>
  invoke<[number, number]>("save_pdf_reduced", { pages, dest, level, fills });
export const retouchPicture = (doc: number, retouch: Retouch) => invoke<DocInfo>("retouch_picture", { doc, retouch });
/** Aperçu rapide avec Espace (Windows) : null quand l'option n'existe pas. */
export const apercuRapideEnabled = () => invoke<boolean | null>("apercu_rapide_enabled");
export const setApercuRapide = (enabled: boolean) => invoke<void>("set_apercu_rapide", { enabled });
/** Le fichier à montrer si cette fenêtre est un aperçu rapide. */
export const apercuRapidePath = () => invoke<string | null>("apercu_rapide_path");
export const openInApp = (path: string) => invoke<void>("open_in_app", { path });
export const formFields = (doc: number) => invoke<FormField[]>("form_fields", { doc });
/** Les annotations déjà dans un PDF, page par page, reprises pour être modifiables. */
export const pdfAnnotations = (doc: number) => invoke<unknown[][]>("pdf_annotations", { doc });

export const savePdf = (pages: PageRef[], dest: string, fills: DocFill[] = []) =>
  invoke<void>("save_pdf", { pages, dest, fills });
export const savePicture = (doc: number, rotation: number, dest: string) =>
  invoke<void>("save_picture", { doc, rotation, dest });
/** Les fichiers à ouvrir dans cette fenêtre (au lancement, ou envoyés ensuite par Windows). */
export const takePaths = () => invoke<string[]>("take_paths");
/** Les images du dossier de cette image, dans l'ordre de l'Explorateur. */
export const folderImages = (path: string) => invoke<string[]>("folder_images", { path });
/** Copie des fichiers (comme dans l'Explorateur) ; `add` : en plus de ceux déjà copiés. Renvoie le total. */
export const copyFiles = (paths: string[], add: boolean) => invoke<number>("copy_files", { paths, add });
/** Ouvre les réglages Windows des applis par défaut, sur l'appli. */
export const chooseDefaultApp = () => invoke<void>("choose_default_app");

const isWindows = navigator.userAgent.includes("Windows");

/** Vignette d'une image du disque (au plus `size` pixels de côté). */
export function thumbUrl(path: string, size: number): string {
  const rest = `${size}/${encodeURIComponent(path)}`;
  return isWindows ? `http://vignette.localhost/${rest}` : `vignette://localhost/${rest}`;
}

/** Adresse de l'image d'une page, rendue par le cœur à la largeur demandée. */
export function pageUrl(doc: number, page: number, width: number, rotation: number, withoutFields = false): string {
  const path = `${doc}/${page}/${width}/${rotation}${withoutFields ? "/sans-champs" : ""}`;
  return isWindows ? `http://page.localhost/${path}` : `page://localhost/${path}`;
}

import { invoke } from "./invoke";

// Sanitized HTML from the backend renderer: safe tags and web links only.
export const renderMarkdown = (markdown: string) => invoke<string>("render_markdown", { markdown });

import { useState, useEffect, useCallback } from "react";
import {
  Document,
  Close,
  ChevronDown,
  ChevronRight,
} from "@carbon/icons-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkBreaks from "remark-breaks";
import { fsApi } from "@/api/client";
import { MermaidBlock } from "./MessageBubble";
import type { FsFileContent } from "@/types";
import styles from "./FileExplorerModal.module.css";

export interface FileExplorerModalProps {
  isOpen: boolean;
  initialPath: string;
  onClose: () => void;
}

interface TreeNode {
  path: string;
  name: string;
  kind: "file" | "dir";
  extension: string | null;
  isExpanded: boolean;
  isLoading: boolean;
  childPaths: string[] | null;
  loadError: string | null;
}

// ── Sub-components ────────────────────────────────────────────────────────────

interface TreeEntryProps {
  path: string;
  depth: number;
  nodes: Record<string, TreeNode>;
  selectedPath: string | null;
  onSelectFile: (path: string) => void;
  onToggleDir: (path: string) => void;
}

function TreeEntry({
  path,
  depth,
  nodes,
  selectedPath,
  onSelectFile,
  onToggleDir,
}: TreeEntryProps) {
  const node = nodes[path];
  if (!node) return null;
  const isSelected = path === selectedPath;
  const indent = depth * 16;

  return (
    <div>
      <div
        style={{ paddingLeft: indent }}
        className={`${styles.treeRow} ${isSelected ? styles.treeRowSelected : ""}`}
        onClick={() => {
          if (node.kind === "dir") onToggleDir(path);
          else onSelectFile(path);
        }}
      >
        {node.kind === "dir" ? (
          <span className={styles.treeArrow}>
            {node.isExpanded ? (
              <ChevronDown size={14} />
            ) : (
              <ChevronRight size={14} />
            )}
          </span>
        ) : (
          <span className={styles.treeFileIcon}>
            <Document size={14} />
          </span>
        )}
        <span className={styles.treeName}>{node.name}</span>
        {node.isLoading && <span className={styles.treeLoading}>…</span>}
      </div>
      {node.kind === "dir" && node.isExpanded && (
        <div>
          {node.loadError ? (
            <div
              style={{ paddingLeft: indent + 16 }}
              className={styles.treeAccessError}
            >
              🔒 Not accessible through agent-deck
            </div>
          ) : node.childPaths ? (
            <>
              {node.childPaths.map((childPath) => (
                <TreeEntry
                  key={childPath}
                  path={childPath}
                  depth={depth + 1}
                  nodes={nodes}
                  selectedPath={selectedPath}
                  onSelectFile={onSelectFile}
                  onToggleDir={onToggleDir}
                />
              ))}
              {node.childPaths.length === 0 && (
                <div
                  style={{ paddingLeft: indent + 16 }}
                  className={styles.treeEmpty}
                >
                  Empty directory
                </div>
              )}
            </>
          ) : null}
        </div>
      )}
    </div>
  );
}

interface BreadcrumbProps {
  path: string;
  isFilePath?: boolean;
  onNavigate: (path: string) => void;
}

function Breadcrumb({ path, isFilePath, onNavigate }: BreadcrumbProps) {
  const parts = path.split("/").filter(Boolean);
  return (
    <div className={styles.breadcrumb}>
      <button
        className={styles.breadcrumbSegment}
        onClick={() => onNavigate("/")}
      >
        /
      </button>
      {parts.map((part, i) => {
        const href = "/" + parts.slice(0, i + 1).join("/");
        const isLast = i === parts.length - 1;
        return (
          <span key={href}>
            <span className={styles.breadcrumbSep}>/</span>
            {isLast && isFilePath ? (
              <span className={styles.breadcrumbCurrent}>{part}</span>
            ) : (
              <button
                className={styles.breadcrumbSegment}
                onClick={() => onNavigate(href)}
              >
                {part}
              </button>
            )}
          </span>
        );
      })}
    </div>
  );
}

interface PreviewPaneProps {
  data: FsFileContent | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
}

function PreviewPane({ data, loading, error, onRetry }: PreviewPaneProps) {
  if (loading) {
    return <div className={styles.previewLoading}>Loading…</div>;
  }
  if (error) {
    return (
      <div className={styles.previewError}>
        {error}
        <button onClick={onRetry} className={styles.retryBtn}>
          Retry
        </button>
      </div>
    );
  }
  if (!data) {
    return <div className={styles.previewEmpty}>Select a file to preview</div>;
  }
  if (!data.previewable) {
    return (
      <div className={styles.previewNonPreviewable}>
        <div className={styles.previewFileName}>
          {data.path.split("/").pop()}
        </div>
        <div className={styles.previewReason}>
          {data.reason === "too_large"
            ? "File is too large to preview (> 5 MB)"
            : "Binary file — cannot preview"}
        </div>
        <div className={styles.previewSize}>
          {(data.size / 1024).toFixed(1)} KB
        </div>
      </div>
    );
  }
  if (data.is_image && data.image_data) {
    return (
      <img
        src={`data:image/${data.extension ?? "png"};base64,${data.image_data}`}
        alt={data.path.split("/").pop()}
        className={styles.previewImage}
      />
    );
  }
  const ext = data.extension?.toLowerCase();
  if (ext === "md" && data.content) {
    return (
      <div className={styles.previewMarkdown}>
        <ReactMarkdown
          remarkPlugins={[remarkGfm, remarkBreaks]}
          components={{
            code({
              className,
              children,
              ...props
            }: React.HTMLAttributes<HTMLElement>) {
              const language = /language-(\w+)/.exec(className ?? "")?.[1];
              if (language === "mermaid") {
                return (
                  <MermaidBlock source={String(children).replace(/\n$/, "")} />
                );
              }
              return (
                <code className={className} {...props}>
                  {children}
                </code>
              );
            },
          }}
        >
          {data.content}
        </ReactMarkdown>
      </div>
    );
  }
  return (
    <div className={styles.previewCode}>
      <div className={styles.previewCodeLang}>{ext ?? "text"}</div>
      <pre className={styles.previewCodePre}>{data.content}</pre>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function FileExplorerModal({
  isOpen,
  initialPath,
  onClose,
}: FileExplorerModalProps) {
  const [nodes, setNodes] = useState<Record<string, TreeNode>>({});
  const [rootPath, setRootPath] = useState<string>("");
  const [rootEntries, setRootEntries] = useState<string[]>([]);
  const [treeLoadError, setTreeLoadError] = useState<string | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [previewData, setPreviewData] = useState<FsFileContent | null>(null);
  const [previewFading, setPreviewFading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [mobilePanel, setMobilePanel] = useState<"tree" | "preview">("tree");
  const [isMobile, setIsMobile] = useState(
    () => window.matchMedia("(max-width: 768px)").matches,
  );

  // Resize listener for mobile detection
  useEffect(() => {
    const handler = () => {
      setIsMobile(window.matchMedia("(max-width: 768px)").matches);
    };
    window.addEventListener("resize", handler);
    return () => window.removeEventListener("resize", handler);
  }, []);

  // Escape key closes modal
  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isOpen, onClose]);

  const loadDir = useCallback(async (path: string): Promise<string[]> => {
    const res = await fsApi.list(path);
    const entries = res.data.entries;
    setNodes((prev) => {
      const next = { ...prev };
      for (const entry of entries) {
        next[entry.path] = {
          path: entry.path,
          name: entry.name,
          kind: entry.kind,
          extension: entry.extension,
          isExpanded: false,
          isLoading: false,
          childPaths: entry.kind === "dir" ? null : [],
          loadError: null,
        };
      }
      if (next[path]) {
        next[path] = {
          ...next[path],
          childPaths: entries.map((e) => e.path),
          isLoading: false,
          loadError: null,
        };
      }
      return next;
    });
    return entries.map((e) => e.path);
  }, []);

  const handleSelectFile = useCallback(async (path: string) => {
    setSelectedPath(path);
    setPreviewFading(true);
    setPreviewError(null);
    setMobilePanel("preview");
    // Keep old previewData visible during load — cleared only on error or
    // when no previous content exists, to avoid a jarring blank flash.
    try {
      const res = await fsApi.read(path);
      setPreviewData(res.data);
    } catch {
      setPreviewData(null);
      setPreviewError("Failed to load file.");
    } finally {
      setPreviewFading(false);
    }
  }, []);

  const handleToggleDir = useCallback(
    async (path: string) => {
      const node = nodes[path];
      if (!node || node.kind !== "dir") return;
      if (node.isExpanded) {
        setNodes((prev) => ({
          ...prev,
          [path]: { ...prev[path], isExpanded: false },
        }));
        return;
      }
      if (node.childPaths === null) {
        setNodes((prev) => ({
          ...prev,
          [path]: { ...prev[path], isLoading: true, loadError: null },
        }));
        try {
          await loadDir(path);
          setNodes((prev) => ({
            ...prev,
            [path]: { ...prev[path], isExpanded: true, isLoading: false },
          }));
        } catch {
          setNodes((prev) => ({
            ...prev,
            [path]: {
              ...prev[path],
              isExpanded: true,
              isLoading: false,
              loadError: "Not accessible",
            },
          }));
        }
      } else {
        setNodes((prev) => ({
          ...prev,
          [path]: { ...prev[path], isExpanded: true },
        }));
      }
    },
    [nodes, loadDir],
  );

  const navigateToDir = useCallback(
    (newPath: string) => {
      setRootPath(newPath);
      setSelectedPath(null);
      setPreviewData(null);
      setPreviewError(null);
      setTreeLoadError(null);
      setNodes({
        [newPath]: {
          path: newPath,
          name: newPath.split("/").pop() ?? newPath,
          kind: "dir",
          extension: null,
          isExpanded: true,
          isLoading: true,
          childPaths: null,
          loadError: null,
        },
      });
      setRootEntries([]);
      loadDir(newPath)
        .then((paths) => {
          setRootEntries(paths);
        })
        .catch(() => {
          setTreeLoadError(
            "This location is not accessible through agent-deck.",
          );
        });
    },
    [loadDir],
  );

  // Initialization: load tree root whenever the modal opens or initialPath changes
  useEffect(() => {
    if (!isOpen) return;

    const lastSegment = initialPath.split("/").pop() ?? "";
    const isFile = lastSegment.includes(".") && !initialPath.endsWith("/");
    const rootDir = isFile
      ? initialPath.split("/").slice(0, -1).join("/") || "/"
      : initialPath;

    setRootPath(rootDir);
    setSelectedPath(null);
    setPreviewData(null);
    setPreviewError(null);
    setTreeLoadError(null);
    setNodes({
      [rootDir]: {
        path: rootDir,
        name: rootDir.split("/").pop() ?? rootDir,
        kind: "dir",
        extension: null,
        isExpanded: true,
        isLoading: true,
        childPaths: null,
        loadError: null,
      },
    });
    setRootEntries([]);

    loadDir(rootDir)
      .then((paths) => {
        setRootEntries(paths);
        if (isFile) {
          void handleSelectFile(initialPath);
          if (window.matchMedia("(max-width: 768px)").matches) {
            setMobilePanel("preview");
          }
        }
      })
      .catch(() => {
        setTreeLoadError("This location is not accessible through agent-deck.");
      });
  }, [isOpen, initialPath, loadDir, handleSelectFile]);

  if (!isOpen) return null;

  return (
    <div
      className={styles.overlay}
      onClick={(e) => {
        if (!isMobile && e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className={`${styles.dialog} ${isMobile ? styles.dialogMobile : ""}`}
      >
        {/* Header */}
        <div className={styles.dialogHeader}>
          {isMobile && mobilePanel === "preview" ? (
            <button
              className={styles.backBtn}
              onClick={() => setMobilePanel("tree")}
            >
              ← Back
            </button>
          ) : (
            <span className={styles.dialogTitle}>File Explorer</span>
          )}
          <button
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Close"
          >
            <Close size={16} />
          </button>
        </div>

        {/* Body */}
        <div className={`${styles.body} ${isMobile ? styles.bodyMobile : ""}`}>
          {/* Tree panel */}
          {(!isMobile || mobilePanel === "tree") && (
            <div className={styles.treePanel}>
              <Breadcrumb
                path={selectedPath ?? rootPath}
                isFilePath={selectedPath !== null}
                onNavigate={navigateToDir}
              />
              <div className={styles.treeScroll}>
                {treeLoadError ? (
                  <div className={styles.treeAccessError}>
                    🔒 {treeLoadError}
                  </div>
                ) : (
                  rootEntries.map((path) => (
                    <TreeEntry
                      key={path}
                      path={path}
                      depth={0}
                      nodes={nodes}
                      selectedPath={selectedPath}
                      onSelectFile={handleSelectFile}
                      onToggleDir={handleToggleDir}
                    />
                  ))
                )}
              </div>
            </div>
          )}

          {/* Preview panel */}
          {(!isMobile || mobilePanel === "preview") && (
            <div className={styles.previewPanel}>
              {selectedPath && (
                <div className={styles.previewPaneHeader}>
                  <span className={styles.previewPaneFilename}>
                    {selectedPath.split("/").pop()}
                  </span>
                  <button
                    className={styles.downloadBtn}
                    title="Download file"
                    onClick={() => {
                      const url = `/api/fs/download?path=${encodeURIComponent(selectedPath)}`;
                      const anchor = document.createElement("a");
                      anchor.href = url;
                      anchor.download =
                        selectedPath.split("/").pop() ?? "download";
                      document.body.appendChild(anchor);
                      anchor.click();
                      document.body.removeChild(anchor);
                    }}
                  >
                    ⬇
                  </button>
                </div>
              )}
              <div
                className={
                  previewFading && previewData
                    ? styles.previewContentFading
                    : styles.previewContent
                }
              >
                <PreviewPane
                  data={previewData}
                  loading={previewFading && !previewData}
                  error={previewError}
                  onRetry={() =>
                    selectedPath && void handleSelectFile(selectedPath)
                  }
                />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

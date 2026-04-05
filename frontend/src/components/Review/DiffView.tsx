interface Props {
  files: { name: string; additions: number; deletions: number }[];
}

export function DiffView({ files }: Props) {
  if (files.length === 0) {
    return (
      <div className="text-xs text-slate-600 italic p-4 border border-dashed border-slate-800 rounded-lg text-center">
        No file changes to display.
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {files.map((file) => (
        <div
          key={file.name}
          className="flex items-center justify-between p-3 bg-black/40 border border-slate-800 rounded-lg"
        >
          <span className="text-xs font-mono text-slate-300">{file.name}</span>
          <div className="flex gap-3 text-[10px] font-mono">
            <span className="text-emerald-400">+{file.additions}</span>
            <span className="text-red-400">-{file.deletions}</span>
          </div>
        </div>
      ))}
    </div>
  );
}

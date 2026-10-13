import { memo, useCallback, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Check, Copy, Download, Trash2, Upload } from "lucide-react";
import { cn } from "cn";

import { commands, type CustomTheme } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ThemeEditor } from "@/settings/ThemeEditor";
import { applyTheme } from "@/theme/apply";
import { BUILTIN_THEMES, defaultThemeFor } from "@/theme/builtins";
import {
  allThemeNames,
  customTheme,
  duplicate,
  findTheme,
  newThemeId,
  parseThemeFile,
  resolveAppearance,
  SYSTEM,
  themeFile,
  uniqueName,
  type Appearance,
  type Theme,
} from "@/theme/theme";
import type { ThemeBase, ThemeColors } from "@/theme/tokens";

function Swatch({ colors }: { colors: ThemeColors }) {
  return (
    <div
      className="relative h-full w-full"
      style={{ background: colors.background }}
    >
      <div
        className="absolute inset-y-0 right-0 w-[30%]"
        style={{
          background: colors.surface,
          borderLeft: `1px solid ${colors.border}`,
        }}
      >
        <div
          className="mx-auto mt-2.5 size-3 rounded-full"
          style={{ background: colors.accent }}
        />
        <div
          className="mx-auto mt-2 h-2.5 w-[60%] rounded-sm"
          style={{ background: colors.primary }}
        />
      </div>
      <div
        className="absolute top-2.5 left-2.5 h-1.5 w-[38%] rounded-full"
        style={{ background: colors.foreground }}
      />
      <div
        className="absolute top-5 left-2.5 h-1.5 w-[24%] rounded-full"
        style={{ background: colors.mutedForeground }}
      />
      <div className="absolute right-[34%] bottom-2.5 left-2.5 flex h-2 gap-1">
        {[colors.clip, colors.zoom, colors.annotation].map((fill, i) => (
          <div
            key={i}
            className="flex-1 rounded-sm"
            style={{ background: fill }}
          />
        ))}
      </div>
    </div>
  );
}

const ThemeCard = memo(function ThemeCard({
  id,
  name,
  colors,
  darkColors,
  selected,
  onSelect,
}: {
  id: string;
  name: string;
  colors: ThemeColors;
  /** For System, the dark half of the swatch. */
  darkColors?: ThemeColors;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      aria-label={name}
      data-theme-card={id}
      className={cn(
        "bg-card focus-visible:ring-ring/50 overflow-hidden rounded-lg border text-left outline-none focus-visible:ring-[3px]",
        selected && "ring-brand ring-2",
      )}
      onClick={() => onSelect(id)}
    >
      <div className="flex h-16">
        <div className="flex-1 overflow-hidden">
          <Swatch colors={colors} />
        </div>
        {darkColors && (
          <div className="flex-1 overflow-hidden">
            <Swatch colors={darkColors} />
          </div>
        )}
      </div>
      <div className="flex items-center justify-between gap-1 border-t px-2 py-1.5 text-xs">
        <span className="truncate">{name}</span>
        {selected && <Check className="text-brand size-3.5 shrink-0" />}
      </div>
    </button>
  );
});

function ThemeSelect({
  label,
  base,
  value,
  themes,
  onChange,
}: {
  label: string;
  base: ThemeBase;
  value: string;
  themes: Theme[];
  onChange: (id: string) => void;
}) {
  const options = themes.filter((t) => t.base === base);
  const shown = options.some((t) => t.id === value)
    ? value
    : defaultThemeFor(base).id;
  return (
    <div className="flex items-center justify-between gap-4">
      <Label className="font-normal">{label}</Label>
      <Select value={shown} onValueChange={onChange}>
        <SelectTrigger size="sm" className="w-48" aria-label={label}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent position="popper">
          {options.map((t) => (
            <SelectItem key={t.id} value={t.id}>
              {t.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

type Notice = { kind: "error" | "info"; text: string };

const fileName = (path: string) => path.split("/").pop() ?? path;

/**
 * Picks, copies, edits, imports and exports themes. Changes show in every window at
 * once.
 */
export function AppearanceSettings({
  appearance,
  onChange,
}: {
  appearance: Appearance;
  onChange: (appearance: Appearance) => void;
}) {
  const [notice, setNotice] = useState<Notice | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const customs = appearance.customThemes;
  const themes = [...BUILTIN_THEMES, ...customs.map(customTheme)];
  const selected = findTheme(appearance.theme, customs);
  const custom = customs.find((t) => t.id === appearance.theme);

  // Sent at once: Rust applies changes in order, restyles the other windows and
  // writes the settings file once edits pause, or on quit.
  const saved = useRef(appearance);
  const sent = useRef(0);
  const change = (next: Omit<Appearance, "applied">) => {
    const applied = resolveAppearance({ ...next, applied: null });
    const full = { ...next, applied };
    applyTheme(applied);
    onChange(full);
    const id = ++sent.current;
    void commands.setAppearance(full).then((result) => {
      if (result.status === "ok") {
        saved.current = full;
        return;
      }
      setNotice({ kind: "error", text: result.error });
      // Rust kept the last change it accepted; show that again unless newer ones follow.
      if (id === sent.current) {
        applyTheme(saved.current.applied);
        onChange(saved.current);
      }
    });
  };

  const select = (id: string) => {
    setConfirmDelete(null);
    if (id !== appearance.theme) change({ ...appearance, theme: id });
  };

  const updateLatest = useRef<(theme: CustomTheme) => void>(() => {});
  updateLatest.current = (theme) =>
    change({
      ...appearance,
      customThemes: customs.map((t) => (t.id === theme.id ? theme : t)),
    });
  const updateCustom = useCallback(
    (theme: CustomTheme) => updateLatest.current(theme),
    [],
  );

  const duplicateSelected = () => {
    if (!selected) return;
    const copy = duplicate(selected, customs);
    setNotice(null);
    change({ ...appearance, theme: copy.id, customThemes: [...customs, copy] });
  };

  const remove = (theme: CustomTheme) => {
    if (confirmDelete !== theme.id) {
      setConfirmDelete(theme.id);
      return;
    }
    setConfirmDelete(null);
    const fallback = (id: string, base: ThemeBase) =>
      id === theme.id ? defaultThemeFor(base).id : id;
    const replacement = BUILTIN_THEMES.some(
      (t) => t.id === theme.basedOn && t.base === theme.base,
    )
      ? theme.basedOn!
      : defaultThemeFor(theme.base).id;
    change({
      ...appearance,
      theme: appearance.theme === theme.id ? replacement : appearance.theme,
      systemLight: fallback(appearance.systemLight, "light"),
      systemDark: fallback(appearance.systemDark, "dark"),
      customThemes: customs.filter((t) => t.id !== theme.id),
    });
    setNotice({ kind: "info", text: `Deleted “${theme.name}”.` });
  };

  const importTheme = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      title: "Import Theme",
      filters: [{ name: "Recast Theme", extensions: ["json"] }],
    });
    if (typeof path !== "string") return;
    const read = await commands.importTheme(path);
    if (read.status === "error") {
      setNotice({ kind: "error", text: read.error });
      return;
    }
    const parsed = parseThemeFile(read.data);
    if (!parsed.ok) {
      setNotice({
        kind: "error",
        text: `Couldn't import ${fileName(path)}. ${parsed.error}`,
      });
      return;
    }
    const theme: CustomTheme = {
      id: newThemeId(),
      name: uniqueName(parsed.name, allThemeNames(customs)),
      base: parsed.base,
      basedOn: null,
      colors: parsed.colors,
    };
    change({
      ...appearance,
      theme: theme.id,
      customThemes: [...customs, theme],
    });
    setNotice({
      kind: "info",
      text: [`Imported “${theme.name}”.`, ...parsed.warnings].join(" "),
    });
  };

  const exportSelected = async () => {
    if (!selected) return;
    const path = await save({
      title: "Export Theme",
      defaultPath: `${selected.name}.json`,
      filters: [{ name: "Recast Theme", extensions: ["json"] }],
    });
    if (!path) return;
    const result = await commands.exportTheme(path, themeFile(selected));
    setNotice(
      result.status === "ok"
        ? {
            kind: "info",
            text: `Exported “${selected.name}” to ${fileName(path)}.`,
          }
        : { kind: "error", text: result.error },
    );
  };

  const systemLight =
    findTheme(appearance.systemLight, customs) ?? defaultThemeFor("light");
  const systemDark =
    findTheme(appearance.systemDark, customs) ?? defaultThemeFor("dark");
  const card = (theme: Theme) => (
    <ThemeCard
      key={theme.id}
      id={theme.id}
      name={theme.name}
      colors={theme.colors}
      selected={appearance.theme === theme.id}
      onSelect={select}
    />
  );

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <div className="space-y-0.5">
          <Label>Theme</Label>
          <p className="text-muted-foreground text-xs">
            Colors every Recast window. Your recordings, screenshots and marks
            keep their own colors.
          </p>
        </div>
        <div
          role="group"
          aria-label="Themes"
          className="grid grid-cols-2 gap-2 sm:grid-cols-3"
        >
          <ThemeCard
            id={SYSTEM}
            name="System"
            colors={systemLight.colors}
            darkColors={systemDark.colors}
            selected={appearance.theme === SYSTEM}
            onSelect={select}
          />
          {BUILTIN_THEMES.map(card)}
        </div>
      </div>

      {customs.length > 0 && (
        <div className="space-y-2">
          <Label>Your Themes</Label>
          <div
            role="group"
            aria-label="Your themes"
            className="grid grid-cols-2 gap-2 sm:grid-cols-3"
          >
            {customs.map(customTheme).map(card)}
          </div>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={!selected}
          onClick={duplicateSelected}
        >
          <Copy /> Duplicate
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={!selected}
          onClick={() => void exportSelected()}
        >
          <Download /> Export…
        </Button>
        <Button size="sm" variant="outline" onClick={() => void importTheme()}>
          <Upload /> Import…
        </Button>
        {custom && (
          <Button
            size="sm"
            variant={confirmDelete === custom.id ? "destructive" : "ghost"}
            onClick={() => remove(custom)}
          >
            <Trash2 />
            {confirmDelete === custom.id ? "Click Again to Delete" : "Delete"}
          </Button>
        )}
      </div>

      {notice && (
        <p
          role={notice.kind === "error" ? "alert" : "status"}
          className={cn(
            "text-xs",
            notice.kind === "error"
              ? "text-destructive"
              : "text-muted-foreground",
          )}
        >
          {notice.text}
        </p>
      )}

      {appearance.theme === SYSTEM && (
        <div className="space-y-2 rounded-lg border p-3">
          <p className="text-muted-foreground text-xs">
            Follows your Mac’s light or dark appearance.
          </p>
          <ThemeSelect
            label="Light appearance"
            base="light"
            value={appearance.systemLight}
            themes={themes}
            onChange={(systemLight) => change({ ...appearance, systemLight })}
          />
          <ThemeSelect
            label="Dark appearance"
            base="dark"
            value={appearance.systemDark}
            themes={themes}
            onChange={(systemDark) => change({ ...appearance, systemDark })}
          />
        </div>
      )}

      {selected?.builtin && (
        <p className="text-muted-foreground text-xs">
          Built-in themes can’t be changed. Duplicate one to make your own.
        </p>
      )}

      {custom && (
        <ThemeEditor
          key={custom.id}
          theme={custom}
          otherNames={themes
            .filter((t) => t.id !== custom.id)
            .map((t) => t.name)}
          onChange={updateCustom}
        />
      )}
    </div>
  );
}

import type { Ref } from "react";
import type { UrlHit } from "../api";
import { copyText, openUrl } from "../api";
import { DataGrid, type Column } from "./DataGrid";
import { useDialog } from "./Dialog";
import type { ListHandle } from "./VirtualList";
import { errText } from "./util";

/** URL rendered as a link that opens in the default browser (never navigates the webview). */
export function UrlLink({ url }: { url: string }) {
  const { showError } = useDialog();
  return (
    <a
      href={url}
      className="link"
      onClick={(e) => {
        e.preventDefault();
        openUrl(url).catch((err) => showError(errText(err)));
      }}
    >
      {url}
    </a>
  );
}

/** Grid of URL events with link cells, Ctrl+C = copy URL only, and a copy context menu. */
export function UrlGrid({
  rows,
  withWorld,
  onActivate,
  className,
  ariaLabel,
  ref,
}: {
  rows: UrlHit[];
  withWorld: boolean;
  onActivate?: (row: UrlHit) => void;
  className?: string;
  ariaLabel: string;
  ref?: Ref<ListHandle>;
}) {
  const { showError } = useDialog();
  const copy = (text: string) => copyText(text).catch((e) => showError(errText(e)));

  const urlCol: Column<UrlHit> = {
    header: "URL",
    flex: 1,
    text: (r) => r.url,
    render: (r) => <UrlLink url={r.url} />,
  };
  const columns: Column<UrlHit>[] = withWorld
    ? [
        { header: "時刻", width: 150, text: (r) => r.eventTime },
        { header: "種別", width: 60, text: (r) => r.kind },
        { header: "ワールド", width: 200, text: (r) => r.worldName },
        urlCol,
      ]
    : [{ header: "時刻", width: 150, text: (r) => r.eventTime }, { header: "種別", width: 60, text: (r) => r.kind }, urlCol];

  return (
    <DataGrid<UrlHit>
      ref={ref}
      className={className}
      ariaLabel={ariaLabel}
      items={rows}
      getKey={(_, i) => i}
      columns={columns}
      onActivate={onActivate}
      onCopy={(r) => void copy(r.url)}
      getMenu={(r) => [
        { label: "URL をコピー", hint: "Ctrl+C", onClick: () => void copy(r.url) },
        {
          label: "行をコピー (時刻・種別・URL)",
          onClick: () => void copy(`${r.eventTime}\t${r.kind}\t${r.worldName}\t${r.url}`),
        },
      ]}
    />
  );
}

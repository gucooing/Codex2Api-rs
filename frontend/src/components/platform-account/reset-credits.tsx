"use client";
import { GrantResetCreditsDialog } from "@/components/platform-account/grant-reset-credits-dialog";
import { ResetCreditDialog } from "@/components/platform-account/reset-credit-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardDescription } from "@/components/ui/card";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { date } from "@/lib/format";
import { useConsumerResetCredits } from "@/lib/platform-account";
import { Columns3 } from "lucide-react";

export function ConsumerResetCredits({ id }: { id: string }) {
  const {
    tableColumns1,
    fieldId,
    dialogFocus,
    grantOpen,
    setGrantOpen,
    startMode,
    setStartMode,
    path,
    resource,
    actions,
    quantity,
    setQuantity,
    note,
    setNote,
    activateAt,
    setActivateAt,
    durationDays,
    setDurationDays,
    busy,
    rows,
    handleClick,
  } = useConsumerResetCredits({ id });
  return (
    <>
      <div className="flex items-center gap-3">
        <Button
          size="sm"
          disabled={!resource.ready || busy}
          onClick={() => {
            setStartMode("now");
            setActivateAt("");
            setDurationDays("30");
            setQuantity("1");
            setNote("");
            setGrantOpen(true);
          }}
        >
          发放重置卡
        </Button>
        <Badge variant="secondary">可用 {resource.data?.available_count ?? "—"} 张</Badge>
      </div>
      <GrantResetCreditsDialog
        grantOpen={grantOpen}
        busy={busy}
        setGrantOpen={setGrantOpen}
        dialogFocus={dialogFocus}
        actions={actions}
        id={id}
        path={path}
        quantity={quantity}
        note={note}
        startMode={startMode}
        activateAt={activateAt}
        durationDays={durationDays}
        setNote={setNote}
        resource={resource}
        fieldId={fieldId}
        setQuantity={setQuantity}
        setStartMode={setStartMode}
        setActivateAt={setActivateAt}
        setDurationDays={setDurationDays}
      />
      <>
        <div className="mb-2 flex justify-end">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" variant="outline" size="sm" aria-label="显示列">
                <Columns3 />
                显示列
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuLabel>
                {tableColumns1.mobile ? "手机显示列" : "桌面显示列"}
              </DropdownMenuLabel>
              {tableColumns1.labels.map((label) => (
                <DropdownMenuCheckboxItem
                  key={label}
                  checked={tableColumns1.isVisible(label)}
                  disabled={tableColumns1.count === 1 && tableColumns1.isVisible(label)}
                  onSelect={(event) => event.preventDefault()}
                  onCheckedChange={(checked) => tableColumns1.setVisible(label, checked === true)}
                >
                  {label}
                </DropdownMenuCheckboxItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={tableColumns1.showAll}>显示全部列</DropdownMenuItem>
              <DropdownMenuItem onSelect={tableColumns1.reset}>恢复默认列</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        <Table
          className={
            tableColumns1.count > 4
              ? "max-md:table-auto max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
              : "max-md:table-fixed max-md:[&_td]:px-1.5 max-md:[&_td]:py-2 max-md:[&_th]:px-1.5 max-md:[&_th]:text-xs max-md:[&_td]:text-xs"
          }
          role="table"
        >
          <TableHeader>
            <TableRow role="row">
              <TableHead hidden={!tableColumns1.isVisible("类型")} className="">
                类型
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("发放时间")} className="">
                发放时间
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("启用时间")} className="">
                启用时间
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("到期时间")} className="">
                到期时间
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("状态")} className="max-md:w-20">
                状态
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("使用时间")} className="">
                使用时间
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("使用方")} className="">
                使用方
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("重置窗口数")} className="">
                重置窗口数
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("管理备注")} className="">
                管理备注
              </TableHead>
              <TableHead hidden={!tableColumns1.isVisible("操作")} className="max-md:w-20">
                操作
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.rows.map((credit) => (
              <TableRow role="row" key={credit.id}>
                <TableCell
                  hidden={!tableColumns1.isVisible("类型")}
                  className=" max-md:overflow-hidden"
                  data-label="类型"
                  data-compact="true"
                  role="cell"
                >
                  <div className="max-md:hidden">
                    {credit.source === "admin_reset" ? "管理员直接重置" : "重置卡"}
                  </div>
                  <ResetCreditDialog credit={credit} />
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("发放时间")}
                  className=" "
                  data-label="发放时间"
                  role="cell"
                >
                  {date(credit.granted_at)}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("启用时间")}
                  className=" "
                  data-label="启用时间"
                  role="cell"
                >
                  {date(credit.available_at)}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("到期时间")}
                  className=" "
                  data-label="到期时间"
                  role="cell"
                >
                  {date(credit.expires_at)}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("状态")}
                  className=" max-md:overflow-hidden"
                  data-label="状态"
                  data-compact="true"
                  role="cell"
                >
                  <Badge variant="outline">
                    {
                      {
                        available: "可用",
                        redeemed: "已使用",
                        pending: "待启用",
                        expired: "已过期",
                        not_applied: "未执行",
                      }[credit.status]
                    }
                  </Badge>
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("使用时间")}
                  className=" "
                  data-label="使用时间"
                  role="cell"
                >
                  {date(credit.redeemed_at)}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("使用方")}
                  className=" "
                  data-label="使用方"
                  data-compact="true"
                  role="cell"
                >
                  {credit.redeemed_by === "admin"
                    ? "管理员"
                    : credit.redeemed_by === "client"
                      ? "客户端"
                      : "—"}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("重置窗口数")}
                  className=" "
                  data-label="重置窗口数"
                  data-compact="true"
                  role="cell"
                >
                  {credit.status === "redeemed" ? credit.windows_reset : "—"}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("管理备注")}
                  data-label="管理备注"
                  role="cell"
                  className="max-w-64 truncate "
                  title={credit.note}
                >
                  {credit.note || "—"}
                </TableCell>
                <TableCell
                  hidden={!tableColumns1.isVisible("操作")}
                  className=" max-md:[&_button]:h-7 max-md:[&_button]:px-1.5 max-md:[&_button]:text-xs max-md:[&_button]:gap-1 max-md:[&_a]:h-7 max-md:[&_a]:px-1.5 max-md:[&_a]:text-xs max-md:[&_a]:gap-1 max-md:[&>div]:gap-1"
                  data-label="操作"
                  role="cell"
                >
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={
                      !resource.ready ||
                      busy ||
                      credit.status !== "available" ||
                      credit.source !== "card"
                    }
                    onClick={() => handleClick(credit)}
                  >
                    使用
                  </Button>
                </TableCell>
              </TableRow>
            ))}
            {rows.rows.length === 0 && (
              <TableRow role="row">
                <TableCell
                  role="cell"
                  colSpan={tableColumns1.count}
                  className="text-center text-muted-foreground"
                >
                  {resource.loading
                    ? "正在加载重置卡"
                    : resource.data
                      ? "尚未发放重置卡"
                      : "重置卡记录暂不可用"}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </>
      <div className="flex items-center justify-end gap-2">
        <CardDescription>{rows.total} 张</CardDescription>
        <Button size="sm" variant="outline" {...rows.previous}>
          上一页
        </Button>
        <CardDescription>
          {rows.page} / {rows.pages}
        </CardDescription>
        <Button size="sm" variant="outline" {...rows.next}>
          下一页
        </Button>
      </div>
    </>
  );
}

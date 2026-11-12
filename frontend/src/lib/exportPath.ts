/** 拖放事件里的路径写法可能和导出时不同（比如 /var 与 /private/var），只比较 export-xxx/文件名 这两段。 */
export function sameExport(path: string, exported: string) {
  const tail = (value: string) => value.normalize("NFC").split(/[\\/]/).slice(-2).join("/");
  return tail(path) === tail(exported);
}

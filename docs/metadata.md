# 写真のメタデータ

VRChat が写真に入れるメタデータと、VrcLogger がそれをどう読み、どう書き込むかの説明です。

## VRChat の PNG

VRChat は、撮影した PNG に XMP を 1 つだけ入れます。

- 置き場所: `iTXt` チャンク。キーワードは `XML:com.adobe.xmp`。圧縮なし、言語タグは空
- 位置: `IHDR` の直後、画像データ（`IDAT`）より前（チャンクの並びは `IHDR iTXt IDAT IEND`）
- `tEXt` / `zTXt` / `eXIf` / `tIME` / ICC プロファイルは無い
- 撮影日時はファイル名にも入っている（`VRChat_2026-01-10_20-30-00.000_3840x2160.png`。ミリ秒まで）

VRChat が書き出す XMP の形（値は例）:

```xml
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
    <rdf:Description>
      <xmp:CreatorTool>VRChat</xmp:CreatorTool>
      <xmp:Author>（撮影者の表示名）</xmp:Author>
      <xmp:CreateDate>2026-01-10T20:30:00.1234567+09:00</xmp:CreateDate>
      <xmp:ModifyDate>2026-01-10T20:30:00.1234567+09:00</xmp:ModifyDate>
    </rdf:Description>
    <rdf:Description xmlns:tiff="http://ns.adobe.com/tiff/1.0/">
      <tiff:DateTime>2026-01-10T20:30:00.1234567+09:00</tiff:DateTime>
    </rdf:Description>
    <rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/">
      <dc:title>
        <rdf:Alt>
          <rdf:li xml:lang="x-default"></rdf:li>
        </rdf:Alt>
      </dc:title>
    </rdf:Description>
    <rdf:Description xmlns:vrc="http://ns.vrchat.com/vrc/1.0/">
      <vrc:WorldID>wrld_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx</vrc:WorldID>
      <vrc:WorldDisplayName>（ワールド名）</vrc:WorldDisplayName>
      <vrc:AuthorID>usr_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx</vrc:AuthorID>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
```

| 項目 | 中身 |
| --- | --- |
| `xmp:CreatorTool` | 撮影したアプリ。常に `VRChat` |
| `xmp:Author` | 撮影者の表示名 |
| `xmp:CreateDate` / `xmp:ModifyDate` / `tiff:DateTime` | 撮影日時。3 つとも同じ値。ISO 8601 で、秒の小数 7 桁と UTC からの時差付き |
| `dc:title` | 空 |
| `vrc:WorldID` | ワールドの ID（`wrld_` + UUID） |
| `vrc:WorldDisplayName` | ワールド名。`!` が似た別の文字（U+01C3）になっていることがある |
| `vrc:AuthorID` | 撮影者のユーザー ID（`usr_` + UUID） |

- `rdf:Description` に `rdf:about` が無い。名前空間ごとに `rdf:Description` を分けている
- `xpacket`（XMP の前後に付く処理命令）は付かない

### 画像編集ソフトで保存し直した PNG

Photoshop などで保存し直すと、XMP の形が変わります。

- 値が要素ではなく属性で書かれ、`xpacket` で囲まれる
- `xmp:Author` が `dc:creator` に変わり、`tiff:DateTime` が無くなる
- `xmp:ModifyDate` が保存した日時に変わり、`xmp:MetadataDate`・`dc:format`・保存の履歴（`xmpMM:*`）が増える
- `xmp:CreateDate` と `vrc:*` は残る

## VrcLogger が読むもの

写真の読み込み（`crates/core/src/photos/meta.rs`）では、XMP から次の値を読みます。

- 撮影日時（`xmp:CreateDate`）。時差は使わず、書かれている時刻をそのまま PC の時刻として扱う
- ワールド ID、ワールド名、撮影者のユーザー ID（`vrc:*`）

XMP が無い写真は、ファイル名の日時、それも無ければ更新日時から撮影日時を決めます。

## VrcLogger が書き込むもの

「メタデータの書き込み」（`crates/core/src/photos/xmp_write.rs`）は、XMP がまったく無い PNG / JPEG に、ログから分かる値を入れます。

- 置き場所: PNG は `IHDR` の直後の `iTXt` チャンク。JPEG は先頭の APP0 / EXIF の後ろの APP1 セグメント
- 画像データは読み直しも再圧縮もしない。その 1 か所を挿入する以外、ファイルのバイト列は変わらない
- 置き換える前に、挿入した部分を取り除くと元のファイルと完全に一致することを確かめる
- 更新日時は元のまま

書き込む XMP の形（値は例）:

```xml
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
    <rdf:Description>
      <xmp:CreatorTool>VRChat</xmp:CreatorTool>
      <xmp:Author>（自分の表示名）</xmp:Author>
      <xmp:CreateDate>2026-01-10T20:30:00+09:00</xmp:CreateDate>
      <xmp:ModifyDate>2026-01-10T20:30:00+09:00</xmp:ModifyDate>
    </rdf:Description>
    <rdf:Description xmlns:tiff="http://ns.adobe.com/tiff/1.0/">
      <tiff:DateTime>2026-01-10T20:30:00+09:00</tiff:DateTime>
    </rdf:Description>
    <rdf:Description xmlns:vrc="http://ns.vrchat.com/vrc/1.0/">
      <vrc:WorldID>wrld_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx</vrc:WorldID>
      <vrc:WorldDisplayName>（ワールド名）</vrc:WorldDisplayName>
      <vrc:AuthorID>usr_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx</vrc:AuthorID>
    </rdf:Description>
    <rdf:Description xmlns:vrclogger="urn:vrclogger:meta:1.0">
      <vrclogger:Source>log</vrclogger:Source>
      <vrclogger:WrittenAt>2026-11-21T12:00:00+09:00</vrclogger:WrittenAt>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
```

| 項目 | 中身 |
| --- | --- |
| `xmp:CreatorTool` | ファイル名が `VRChat_` か `vrc_` で始まるときだけ `VRChat`。それ以外は書かない |
| `xmp:Author` / `vrc:AuthorID` | ログで分かる自分の表示名とユーザー ID。分からなければ書かない |
| 撮影日時の 3 項目 | ファイル名の日時（秒まで）。時差は、その日時に PC のタイムゾーンで有効だった値 |
| `vrc:WorldID` / `vrc:WorldDisplayName` | 撮影時刻にいたワールド（ログから） |
| `vrclogger:Source` | このアプリが書いた目印。常に `log` |
| `vrclogger:WrittenAt` | 書き込んだ日時 |

その場にいた人は書き込みません。

### 取り除き

「書き込んだものを取り除く」は、`vrclogger:Source` の目印がある XMP だけを取り除きます。取り除いたあとのファイルは、書き込む前と完全に同じになります。VRChat やほかのソフトが書いた XMP には触れません。

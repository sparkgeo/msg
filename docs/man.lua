-- Traditional section headings, including NAME for man database indexing.
function Header(header)
  if header.level == 1 then
    header.content = pandoc.Inlines({
      pandoc.Str(string.upper(pandoc.utils.stringify(header.content)))
    })
  end
  return header
end

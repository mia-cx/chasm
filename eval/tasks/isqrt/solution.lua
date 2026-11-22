for _, n in ipairs({0, 1, 15, 16, 1000000, 2147483647}) do
  local r = 0
  while (r + 1) * (r + 1) <= n do r = r + 1 end
  print(r)
end

for _, n in ipairs({1, 12, 255, 1024, 2147483647}) do
  local ones, length = 0, 0
  while n > 0 do
    ones = ones + n % 2
    length = length + 1
    n = n // 2
  end
  print(ones)
  print(length)
end

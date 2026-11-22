for _, n in ipairs({0, 7, 1234, 90210, 123456789}) do
  local sum, rev = 0, 0
  while n > 0 do
    sum = sum + n % 10
    rev = rev * 10 + n % 10
    n = n // 10
  end
  print(sum)
  print(rev)
end

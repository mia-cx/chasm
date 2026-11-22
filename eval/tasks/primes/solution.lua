for n = 2, 99 do
  local prime = true
  for d = 2, n - 1 do
    if d * d > n then break end
    if n % d == 0 then prime = false break end
  end
  if prime then print(n) end
end

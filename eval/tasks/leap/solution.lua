for _, year in ipairs({1900, 1996, 2000, 2023, 2024, 2100}) do
  print(year)
  if (year % 4 == 0 and year % 100 ~= 0) or year % 400 == 0 then
    print("leap")
  else
    print("common")
  end
end

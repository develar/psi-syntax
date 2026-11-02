static IEnumerable<(string Value, string Description) GetAllValues(Type t)
{
    if (!t.IsEnum)
        throw new ArgumentException("no good");

    return Enum.GetValues(t).Cast<Enum>().Select(e => (e.ToString(), e.ToString()));
}
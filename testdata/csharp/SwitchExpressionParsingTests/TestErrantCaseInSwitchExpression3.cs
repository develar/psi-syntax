{
    var y = x switch
    {
        case 0:
            Goo();
            return Bar;
        case 1:
        {
            Baz();
            throw new Quux();
        }
    };
}
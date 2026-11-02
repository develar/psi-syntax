namespace N;

// This has no type to go into.
event Action BeforeTypeEvent;

record Type
{
}

// Two members that will move into the type above.
private void M()
{
}

public int Prop => 0;

// Following type declaration
class Type
{
}

private Constructor() { }

private int field;
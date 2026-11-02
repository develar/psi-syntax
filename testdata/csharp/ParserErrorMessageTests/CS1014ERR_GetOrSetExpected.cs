using System;

public sealed class Container
{
    public string Prop1 { protected }
    public string Prop2 { get { return null; } protected }
    public string Prop3 { get { return null; } protected set { } protected }
}

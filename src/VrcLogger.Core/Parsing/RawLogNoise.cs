namespace VrcLogger.Core.Parsing;

/// <summary>
/// Log lines that are dropped from raw_events. A pattern matches the start of the
/// message; a pattern beginning with '*' matches anywhere in the message.
/// Deliberately conservative: only high-volume avatar/engine boilerplate is listed,
/// so world-creator Debug.Log output and anything unusual is kept.
/// </summary>
public static class RawLogNoise
{
    public static readonly IReadOnlyList<string> Default =
    [
        "[Behaviour] Using default gesture mask",
        "[Behaviour] Using default fx mask",
        "[Behaviour] Using custom gesture mask",
        "[Behaviour] CacheComponents:",
        "[Behaviour] Avatar is Ready, Initializing",
        "[Behaviour] Initialize ThreePoint Avatar",
        "[Behaviour] Initialize Limb Avatar",
        "[Behaviour] Waiting for network limits.",
        "[Behaviour] Restored player",
        "[Behaviour] All 1 bunches for Player Data Object",
        "[Behaviour] Sanity check",
        "[Behaviour] Configuring remote player",
        "[Behaviour] eyeToNeck:",
        "[Behaviour] Avatar has",
        "[Behaviour] Registering Avatar Interaction",
        "[Behaviour] Drop object:",
        "[Behaviour] Pickup object:",
        "Missing button from radio selector group",
        "Climbing up at speed:",
        "Measure Human Avatar",
        "Found SDK3 avatar descriptor.",
        "Recovered 0 Network IDs from Avatar",
        "UnityPackage: Model failed validation",
        "APIUser: Model failed validation",
        "ApiInventoryItem: Model failed validation",
        "Cyclic dependency found for component",
        "Removing camera from",
        "Setting bool parameter",
        "Setting float parameter",
        "Setting int parameter",
        "Attempted to cache texture already cached",
        "PlayOneShot was called with a null AudioClip.",
        "Unable to add the requested character to font asset",
        "Unloading bundle",
        "[AssetBundleDownloadManager]",
        "[EOSManager]",
        "[AdvertisementForFavoriteBooth]",
        "[TextureManagement]",
        "*Avatar will follow Safety Settings",
        "VP MainMenu",
    ];

    public static bool IsNoise(string message, IReadOnlyList<string> patterns)
    {
        foreach (var p in patterns)
        {
            if (p.Length == 0)
                continue;
            if (p[0] == '*')
            {
                if (p.Length > 1 && message.Contains(p.Substring(1), StringComparison.Ordinal))
                    return true;
            }
            else if (message.StartsWith(p, StringComparison.Ordinal))
                return true;
        }
        return false;
    }
}

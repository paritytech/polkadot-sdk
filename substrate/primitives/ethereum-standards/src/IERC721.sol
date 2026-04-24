// SPDX-License-Identifier: MIT
// Based on OpenZeppelin's ERC-721 interfaces:
//
// IERC721.sol (base ERC-721 interface)
// https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/token/ERC721/IERC721.sol
//
// IERC721Metadata.sol (ERC-721 metadata extension)
// https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/token/ERC721/extensions/IERC721Metadata.sol
//
pragma solidity ^0.8.20;

///
/// @dev Interface combining the ERC-721 Non-Fungible Token Standard and its metadata extension.
/// Note: Due to ABI generation constraints, all interfaces are merged into a single contract.
/// The `safeTransferFrom` overloads are omitted; use `transferFrom` for programmatic transfers.
///
interface IERC721 {
    // ============================================================
    // IERC721 - Base ERC-721 Interface
    // https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/token/ERC721/IERC721.sol
    // ============================================================

    /// @dev Emitted when `tokenId` token is transferred from `from` to `to`.
    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);

    /// @dev Emitted when `owner` enables `approved` to manage the `tokenId` token.
    event Approval(address indexed owner, address indexed approved, uint256 indexed tokenId);

    /// @dev Emitted when `owner` enables or disables (`approved`) `operator` to manage all of its assets.
    event ApprovalForAll(address indexed owner, address indexed operator, bool approved);

    /// @dev Returns the number of tokens in `owner`'s account.
    function balanceOf(address owner) external view returns (uint256 balance);

    /// @dev Returns the owner of the `tokenId` token.
    function ownerOf(uint256 tokenId) external view returns (address owner);

    /// @dev Transfers `tokenId` token from `from` to `to`.
    ///
    /// WARNING: Note that the caller is responsible to confirm that the recipient is capable of
    /// receiving ERC-721 or else they may be permanently lost. Usage of {safeTransferFrom}
    /// prevents loss, though the caller must understand this adds an external call which
    /// potentially creates a reentrancy vulnerability.
    ///
    /// Requirements:
    /// - `from` cannot be the zero address.
    /// - `to` cannot be the zero address.
    /// - `tokenId` token must be owned by `from`.
    /// - If the caller is not `from`, it must be approved to move this token by either
    ///   {approve} or {setApprovalForAll}.
    ///
    /// Emits a {Transfer} event.
    function transferFrom(address from, address to, uint256 tokenId) external;

    /// @dev Gives permission to `to` to transfer `tokenId` token to another account.
    /// The approval is cleared when the token is transferred.
    ///
    /// Only a single account can be approved at a time, so approving the zero address clears
    /// previous approvals.
    function approve(address to, uint256 tokenId) external;

    /// @dev Approve or remove `operator` as an operator for the caller.
    function setApprovalForAll(address operator, bool approved) external;

    /// @dev Returns the account approved for `tokenId` token.
    function getApproved(uint256 tokenId) external view returns (address operator);

    /// @dev Returns if the `operator` is allowed to manage all of the assets of `owner`.
    function isApprovedForAll(address owner, address operator) external view returns (bool);

    // ============================================================
    // IERC721Metadata - ERC-721 Metadata Extension
    // https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/token/ERC721/extensions/IERC721Metadata.sol
    // ============================================================

    /// @dev Returns the token collection name.
    function name() external view returns (string memory);

    /// @dev Returns the token collection symbol.
    function symbol() external view returns (string memory);

    /// @dev Returns the Uniform Resource Identifier (URI) for `tokenId` token.
    function tokenURI(uint256 tokenId) external view returns (string memory);
}

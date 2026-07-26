use super::*;

impl Parser {
    pub(super) fn parse_alter_table_partition_cmd(
        &mut self,
        objtype: ObjectType,
    ) -> PResult<AlterTableCmd> {
        let partition_slot = if objtype == ObjectType::Index {
            completion::GrammarSlot::Index
        } else {
            completion::GrammarSlot::Table
        };
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        match self.peek_kind() {
            TokenKind::Attach => {
                self.advance();
                self.expect(TokenKind::Partition)?;
                let name = Box::new(
                    self.try_parse_qualified_range_var_with_slot(partition_slot)
                        .ok_or_else(|| {
                            self.error_here("ATTACH PARTITION requires a partition name")
                        })?,
                );
                let bound = if objtype == ObjectType::Index {
                    None
                } else {
                    Some(Box::new(self.parse_partition_bound()?))
                };
                cmd.subtype = AlterTableType::AttachPartition;
                cmd.def = Some(Box::new(Node::PartitionCmd(PartitionCmd {
                    node_tag: NodeTag::PartitionCmd,
                    name: Some(name),
                    bound,
                    ..PartitionCmd::default()
                })));
            }
            TokenKind::Detach => {
                self.advance();
                self.expect(TokenKind::Partition)?;
                let name = Box::new(
                    self.try_parse_qualified_range_var_with_slot(partition_slot)
                        .ok_or_else(|| {
                            self.error_here("DETACH PARTITION requires a partition name")
                        })?,
                );
                let (concurrent, finalize) = if self.consume(TokenKind::Concurrently) {
                    (true, false)
                } else if self.consume(TokenKind::Finalize) {
                    (false, true)
                } else {
                    (false, false)
                };
                cmd.subtype = if finalize {
                    AlterTableType::DetachPartitionFinalize
                } else {
                    AlterTableType::DetachPartition
                };
                cmd.def = Some(Box::new(Node::PartitionCmd(PartitionCmd {
                    node_tag: NodeTag::PartitionCmd,
                    name: Some(name),
                    concurrent,
                    ..PartitionCmd::default()
                })));
            }
            TokenKind::Split => {
                self.advance();
                self.expect(TokenKind::Partition)?;
                let name = Box::new(
                    self.try_parse_qualified_range_var_with_slot(partition_slot)
                        .ok_or_else(|| {
                            self.error_here("SPLIT PARTITION requires a partition name")
                        })?,
                );
                self.expect(TokenKind::Into)?;
                self.expect(TokenKind::Char('('))?;
                let mut partlist = Vec::new();
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("SPLIT PARTITION requires at least one partition"));
                }
                while !self.at(TokenKind::Char(')')) {
                    self.expect(TokenKind::Partition)?;
                    let part_name = Box::new(
                        self.try_parse_qualified_range_var_with_slot(partition_slot)
                            .ok_or_else(|| {
                                self.error_here("PARTITION requires a partition name")
                            })?,
                    );
                    let bound = Some(Box::new(self.parse_partition_bound()?));
                    partlist.push(Node::SinglePartitionSpec(SinglePartitionSpec {
                        node_tag: NodeTag::SinglePartitionSpec,
                        name: Some(part_name),
                        bound,
                    }));
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                    if self.at(TokenKind::Char(')')) {
                        return Err(self.error_here("expected a partition after ','"));
                    }
                }
                self.expect(TokenKind::Char(')'))?;
                cmd.subtype = AlterTableType::SplitPartition;
                cmd.def = Some(Box::new(Node::PartitionCmd(PartitionCmd {
                    node_tag: NodeTag::PartitionCmd,
                    name: Some(name),
                    partlist,
                    ..PartitionCmd::default()
                })));
            }
            TokenKind::Merge => {
                self.advance();
                self.expect(TokenKind::Partitions)?;
                self.expect(TokenKind::Char('('))?;
                let mut partlist = Vec::new();
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("MERGE PARTITIONS requires at least one partition"));
                }
                while !self.at(TokenKind::Char(')')) {
                    let part = self
                        .try_parse_qualified_range_var_with_slot(partition_slot)
                        .ok_or_else(|| self.error_here("expected a partition name"))?;
                    partlist.push(Node::RangeVar(part));
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                    if self.at(TokenKind::Char(')')) {
                        return Err(self.error_here("expected a partition after ','"));
                    }
                }
                self.expect(TokenKind::Char(')'))?;
                self.expect(TokenKind::Into)?;
                let name = Box::new(
                    self.try_parse_qualified_range_var_with_slot(partition_slot)
                        .ok_or_else(|| {
                            self.error_here("MERGE PARTITIONS INTO requires a partition name")
                        })?,
                );
                cmd.subtype = AlterTableType::MergePartitions;
                cmd.def = Some(Box::new(Node::PartitionCmd(PartitionCmd {
                    node_tag: NodeTag::PartitionCmd,
                    name: Some(name),
                    partlist,
                    ..PartitionCmd::default()
                })));
            }
            other => {
                return Err(self.error_here(format!(
                    "expected an ALTER TABLE partition command, found {:?}",
                    other
                )));
            }
        }
        Ok(cmd)
    }
}
